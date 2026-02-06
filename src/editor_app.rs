//! Standalone COSMIC editor application.
//! Runs as a separate process, communicates with the applet via pipes.
//! Applet → Editor: stdin (length-prefixed JSON frames).
//! Editor → Applet: dedicated FD 3 pipe (set up by the parent at spawn time).
//! stdout/stderr are left untouched — COSMIC can write to them freely.

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
    content: text_editor::Content,
    original_text: String,
}

pub struct EditorFlags {
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
            EditorToApp::SaveAsNew {
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
        core.window.show_headerbar = true;
        core.window.show_minimize = false;
        core.window.show_maximize = false;
        core.window.header_title = "Clipboard Editor".into();

        Self::send_to_applet(&EditorToApp::Ready);

        let app = EditorApp {
            core,
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
                // No-op: editor stays open on focus loss so user can
                // switch windows to copy text and come back.
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
        let editor = cosmic::widget::text_editor(&self.content)
            .on_action(EditorMsg::EditorAction)
            .wrapping(Wrapping::Word)
            .height(Length::Fill)
            .padding(10);

        container(editor)
            .padding(10)
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
    use std::os::unix::io::{FromRawFd, RawFd};

    /// The well-known FD for Editor → Applet IPC, set up by the parent process.
    const IPC_FD: RawFd = 3;

    // The parent process sets up FD 3 as a dedicated IPC pipe.
    // No stdout manipulation needed — COSMIC can write to stdout freely.
    let ipc_file = unsafe { std::fs::File::from_raw_fd(IPC_FD) };
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

    let content = match init_msg {
        AppToEditor::Init { content, .. } => content,
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
        text: content,
    };

    if let Err(e) = cosmic::app::run::<EditorApp>(settings, flags) {
        eprintln!("Editor app failed: {e}");
        std::process::exit(1);
    }
}
