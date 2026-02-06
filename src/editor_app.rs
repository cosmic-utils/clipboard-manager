//! Standalone COSMIC editor application.
//! Runs as a separate process, communicates with the applet via pipes.
//! stdout is redirected to stderr before COSMIC starts (COSMIC writes to stdout),
//! so IPC uses a dup'd fd saved before the redirect.

use cosmic::app::{Core, Task};
use cosmic::iced::Length;
use cosmic::iced_core::text::Wrapping;
use cosmic::iced_futures::Subscription;
use cosmic::iced_widget::text_editor;
use cosmic::widget::container;
use cosmic::Element;
use std::sync::OnceLock;

use crate::editor_ipc::{self, AppToEditor, EditorToApp};

/// Holds the original stdout pipe fd (saved before COSMIC redirects stdout).
static IPC_WRITER: OnceLock<std::sync::Mutex<std::fs::File>> = OnceLock::new();

const EDITOR_APP_ID: &str = "io.github.cosmic_utils.clipboard-editor";

#[derive(Debug, Clone)]
pub enum EditorMsg {
    EditorAction(text_editor::Action),
    FocusLost,
    IpcMessage(AppToEditor),
    IpcDisconnected,
    CloseWindow,
}

pub struct EditorApp {
    core: Core,
    entry_id: i64,
    content: text_editor::Content,
    original_text: String,
}

pub struct EditorFlags {
    pub entry_id: i64,
    pub mime: String,
    pub text: String,
}

impl EditorApp {
    fn send_to_applet(msg: &EditorToApp) {
        if let Some(writer) = IPC_WRITER.get() {
            let mut guard = writer.lock().unwrap();
            if let Err(e) = editor_ipc::write_frame(&mut *guard, msg) {
                eprintln!("[editor] Failed to send {msg:?}: {e}");
            } else {
                eprintln!("[editor] Sent: {msg:?}");
            }
        } else {
            eprintln!("[editor] IPC_WRITER not initialized, can't send {msg:?}");
        }
    }

    fn save_and_exit(&mut self) -> ! {
        let text = self.content.text();
        let is_dirty = text.trim() != self.original_text.trim();
        eprintln!("[editor] save_and_exit: dirty={is_dirty}, text_len={}, original_len={}", text.len(), self.original_text.len());
        let msg = if is_dirty {
            EditorToApp::SaveFinal {
                entry_id: self.entry_id,
                content: text,
            }
        } else {
            EditorToApp::Closed
        };
        Self::send_to_applet(&msg);
        std::process::exit(0);
    }
}

impl cosmic::Application for EditorApp {
    type Executor = cosmic::executor::Default;
    type Flags = EditorFlags;
    type Message = EditorMsg;
    const APP_ID: &'static str = EDITOR_APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.window.show_headerbar = false;

        Self::send_to_applet(&EditorToApp::Ready);

        let app = EditorApp {
            core,
            entry_id: flags.entry_id,
            content: text_editor::Content::with_text(&flags.text),
            original_text: flags.text,
        };
        (app, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            EditorMsg::EditorAction(action) => {
                self.content.perform(action);
            }
            EditorMsg::FocusLost => {
                self.save_and_exit();
            }
            EditorMsg::CloseWindow => {
                self.save_and_exit();
            }
            EditorMsg::IpcMessage(msg) => match msg {
                AppToEditor::EntryDeleted | AppToEditor::CloseRequested => {
                    Self::send_to_applet(&EditorToApp::Closed);
                    std::process::exit(0);
                }
                AppToEditor::Init { .. } => {}
            },
            EditorMsg::IpcDisconnected => {
                self.save_and_exit();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let title = cosmic::widget::text::title3("Clipboard Manager Editor")
            .width(Length::Fill)
            .align_x(cosmic::iced::alignment::Horizontal::Center);

        let editor = cosmic::widget::text_editor(&self.content)
            .on_action(EditorMsg::EditorAction)
            .wrapping(Wrapping::Word)
            .height(Length::Fill)
            .padding(10);

        cosmic::widget::column()
            .push(container(title).padding([8, 10]))
            .push(
                container(editor)
                    .padding(10)
                    .height(Length::Fill)
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        use cosmic::iced::stream::channel;

        let focus_sub = cosmic::iced_futures::event::listen_with(|event, _status, _id| {
            if let cosmic::iced::Event::Window(
                cosmic::iced_runtime::core::window::Event::Unfocused,
            ) = event
            {
                Some(EditorMsg::FocusLost)
            } else {
                None
            }
        });

        let ipc_sub = Subscription::run_with_id(
            "editor_stdin_ipc",
            channel(8, |mut output| async move {
                use cosmic::iced::futures::SinkExt;
                let (tx, mut rx) = tokio::sync::mpsc::channel::<AppToEditor>(8);

                std::thread::spawn(move || {
                    let stdin = std::io::stdin();
                    let mut locked = stdin.lock();
                    loop {
                        match editor_ipc::read_frame::<AppToEditor>(&mut locked) {
                            Ok(msg) => {
                                if tx.blocking_send(msg).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });

                loop {
                    match rx.recv().await {
                        Some(msg) => {
                            output.send(EditorMsg::IpcMessage(msg)).await.ok();
                        }
                        None => {
                            output.send(EditorMsg::IpcDisconnected).await.ok();
                            futures::future::pending::<()>().await;
                        }
                    }
                }
            }),
        );

        Subscription::batch([focus_sub, ipc_sub])
    }

    fn on_close_requested(
        &self,
        _id: cosmic::iced_runtime::core::window::Id,
    ) -> Option<Self::Message> {
        Some(EditorMsg::CloseWindow)
    }
}

/// Entry point for the editor process.
pub fn run_editor() {
    use std::os::unix::io::{AsRawFd, FromRawFd};

    // Save the original stdout pipe fd for IPC BEFORE COSMIC app init.
    // COSMIC/iced writes to stdout during app startup, which would corrupt
    // our length-prefixed IPC frames. We dup stdout, then redirect fd 1 to stderr.
    let ipc_fd = unsafe { libc::dup(std::io::stdout().as_raw_fd()) };
    if ipc_fd < 0 {
        eprintln!("Failed to dup stdout for IPC");
        std::process::exit(1);
    }
    // Redirect stdout to stderr so COSMIC's writes don't go through the pipe
    unsafe {
        libc::dup2(std::io::stderr().as_raw_fd(), std::io::stdout().as_raw_fd());
    }
    let ipc_file = unsafe { std::fs::File::from_raw_fd(ipc_fd) };
    IPC_WRITER
        .set(std::sync::Mutex::new(ipc_file))
        .expect("IPC_WRITER already set");

    let init_msg = {
        let stdin = std::io::stdin();
        let mut locked = stdin.lock();
        match editor_ipc::read_frame::<AppToEditor>(&mut locked) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to read Init from stdin: {e}");
                std::process::exit(1);
            }
        }
    };

    let (entry_id, mime, content) = match init_msg {
        AppToEditor::Init {
            entry_id,
            mime,
            content,
        } => (entry_id, mime, content),
        other => {
            eprintln!("Expected Init message, got: {other:?}");
            std::process::exit(1);
        }
    };

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(600.0, 500.0))
        .transparent(false)
        .exit_on_close(false);

    let flags = EditorFlags {
        entry_id,
        mime,
        text: content,
    };

    if let Err(e) = cosmic::app::run::<EditorApp>(settings, flags) {
        eprintln!("Editor app failed: {e}");
        std::process::exit(1);
    }
}
