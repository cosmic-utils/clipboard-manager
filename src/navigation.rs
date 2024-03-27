use cosmic::iced::{event, Subscription};

#[derive(Debug, Clone)]
pub enum NavigationMessage {
    Down,
    Up,
    Enter,
    Quit,
}

#[allow(clippy::collapsible_match)]
pub fn sub() -> Subscription<NavigationMessage> {
    cosmic::iced_futures::event::listen_with(|event, status| {
        match status {
            event::Status::Captured => None,
            event::Status::Ignored => {
                match event {
                    event::Event::Keyboard(event) => match event {
                        cosmic::iced::keyboard::Event::KeyReleased { key, .. } => {
                            match key {
                                cosmic::iced::keyboard::Key::Named(named) => match named {
                                    cosmic::iced::keyboard::key::Named::Enter => {
                                        Some(NavigationMessage::Enter)
                                    }

                                    cosmic::iced::keyboard::key::Named::ArrowDown => {
                                        Some(NavigationMessage::Down)
                                    }
                                    cosmic::iced::keyboard::key::Named::ArrowUp => {
                                        Some(NavigationMessage::Up)
                                    }
                                    cosmic::iced::keyboard::key::Named::Escape => {
                                        Some(NavigationMessage::Quit)
                                    }

                                    /*
                                    cosmic::iced::keyboard::key::Named::PageDown => todo!(),
                                    cosmic::iced::keyboard::key::Named::PageUp => todo!(),
                                    cosmic::iced::keyboard::key::Named::Backspace => todo!(),

                                    cosmic::iced::keyboard::key::Named::Clear => todo!(),

                                    cosmic::iced::keyboard::key::Named::Delete => todo!(),

                                    cosmic::iced::keyboard::key::Named::Cancel => todo!(),


                                    cosmic::iced::keyboard::key::Named::Execute => todo!(),
                                    cosmic::iced::keyboard::key::Named::Find => todo!(),

                                    cosmic::iced::keyboard::key::Named::Select => todo!(),

                                    cosmic::iced::keyboard::key::Named::PreviousCandidate => todo!(),

                                    cosmic::iced::keyboard::key::Named::ChannelDown => todo!(),
                                    cosmic::iced::keyboard::key::Named::ChannelUp => todo!(),

                                    cosmic::iced::keyboard::key::Named::Close => todo!(),
                                    cosmic::iced::keyboard::key::Named::Open => todo!(),
                                    cosmic::iced::keyboard::key::Named::GoBack => todo!(),

                                    cosmic::iced::keyboard::key::Named::Exit => todo!(),

                                    cosmic::iced::keyboard::key::Named::FavoriteClear0 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteClear1 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteClear2 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteClear3 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteRecall0 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteRecall1 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteRecall2 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteRecall3 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteStore0 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteStore1 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteStore2 => todo!(),
                                    cosmic::iced::keyboard::key::Named::FavoriteStore3 => todo!(),


                                    cosmic::iced::keyboard::key::Named::NavigateIn => todo!(),
                                    cosmic::iced::keyboard::key::Named::NavigateNext => todo!(),
                                    cosmic::iced::keyboard::key::Named::NavigateOut => todo!(),
                                    cosmic::iced::keyboard::key::Named::NavigatePrevious => todo!(),
                                     */
                                    _ => None,
                                },
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
        }
    })
}
