//! Standalone COSMIC editor application.
//! Runs as a separate process, communicates with the applet via stdin/stdout pipes.

use cosmic::app::{Core, Task};
use cosmic::iced::Length;
use cosmic::iced_core::text::Wrapping;
use cosmic::iced_futures::Subscription;
use cosmic::iced_widget::text_editor;
use cosmic::widget::container;
use cosmic::Element;

use crate::editor_ipc::{self, AppToEditor, EditorToApp};

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
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = editor_ipc::write_frame(&mut lock, msg);
    }

    fn save_and_exit(&mut self) -> ! {
        let text = self.content.text();
        let msg = if text.trim() != self.original_text.trim() {
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
        core.window.show_close = false;
        core.window.show_minimize = false;
        core.window.show_maximize = false;
        core.window.header_title = "Edit".into();

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
