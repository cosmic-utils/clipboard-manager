#![allow(dead_code)]
use std::{
    collections::{HashMap, HashSet},
    io::{self},
    mem::{self},
    os::fd::AsFd,
};

use cosmic::cctk::{
    sctk::reexports::protocols_wlr::data_control::v1::client::{
        zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
        zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
        zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    },
    wayland_client::{
        self, ConnectError, Connection, Dispatch, DispatchError, EventQueue, Proxy,
        delegate_dispatch, event_created_child,
        globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
        protocol::{
            wl_registry::WlRegistry,
            wl_seat::{self, WlSeat},
        },
    },
};
use tokio::net::unix::pipe::Receiver;

/// Seat to operate on.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, PartialOrd, Ord, Default)]
pub enum Seat<'a> {
    /// Operate on one of the existing seats depending on the order returned by the compositor.
    ///
    /// This is perfectly fine when only a single seat is present, so for most configurations.
    #[default]
    Unspecified,
    /// Operate on a seat with the given name.
    Specific(&'a str),
}

#[derive(Default)]
pub struct SeatData {
    pub name: Option<String>,
    pub device: Option<ZwlrDataControlDeviceV1>,
    pub offer: Option<ZwlrDataControlOfferV1>,
    pub primary_offer: Option<ZwlrDataControlOfferV1>,
}

impl SeatData {
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name)
    }

    pub fn set_device(&mut self, device: Option<ZwlrDataControlDeviceV1>) {
        if let Some(old_device) = mem::replace(&mut self.device, device) {
            old_device.destroy();
        }
    }

    pub fn set_offer(&mut self, new_offer: Option<ZwlrDataControlOfferV1>) {
        if let Some(old_offer) = mem::replace(&mut self.offer, new_offer) {
            old_offer.destroy();
        }
    }

    pub fn set_primary_offer(&mut self, new_offer: Option<ZwlrDataControlOfferV1>) {
        if let Some(old_offer) = mem::replace(&mut self.primary_offer, new_offer) {
            old_offer.destroy();
        }
    }
}

pub struct CommonState {
    pub seats: Vec<(WlSeat, SeatData)>,
    pub clipboard_manager: ZwlrDataControlManagerV1,
}

impl CommonState {
    fn get_mut_seat(&mut self, seat: &WlSeat) -> Option<&mut SeatData> {
        self.seats
            .iter_mut()
            .find(|e| &e.0 == seat)
            .map(|e| &mut e.1)
    }
}

impl<S> Dispatch<WlSeat, (), S> for CommonState
where
    S: Dispatch<WlSeat, ()> + AsMut<CommonState>,
{
    fn event(
        parent: &mut S,
        seat: &WlSeat,
        event: <WlSeat as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<S>,
    ) {
        let state = parent.as_mut();

        if let wl_seat::Event::Name { name } = event {
            state.get_mut_seat(seat).unwrap().set_name(name);
        }
    }
}

struct State {
    common: CommonState,
    // The value is the set of MIME types in the offer.
    // TODO: We never remove offers from here, even if we don't use them or after destroying them.
    offers: HashMap<ZwlrDataControlOfferV1, HashSet<String>>,
    got_primary_selection: bool,
    // waker: Waker,
}

delegate_dispatch!(State: [WlSeat: ()] => CommonState);

impl AsMut<CommonState> for State {
    fn as_mut(&mut self) -> &mut CommonState {
        &mut self.common
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, WlSeat> for State {
    fn event(
        state: &mut Self,
        _device: &ZwlrDataControlDeviceV1,
        event: <ZwlrDataControlDeviceV1 as wayland_client::Proxy>::Event,
        seat: &WlSeat,
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                state.offers.insert(id.clone(), HashSet::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                state.common.get_mut_seat(seat).unwrap().set_offer(id);
            }
            zwlr_data_control_device_v1::Event::Finished => {
                // Destroy the device stored in the seat as it's no longer valid.
                state.common.get_mut_seat(seat).unwrap().set_device(None);
            }
            zwlr_data_control_device_v1::Event::PrimarySelection { id } => {
                state.got_primary_selection = true;
                state
                    .common
                    .get_mut_seat(seat)
                    .unwrap()
                    .set_primary_offer(id);
            }
            _ => (),
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: <ZwlrDataControlOfferV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.offers.get_mut(offer).unwrap().insert(mime_type);
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Couldn't connect to the Wayland compositor")]
    WaylandConnection(#[source] ConnectError),

    #[error("Wayland compositor communication error")]
    WaylandCommunication(#[source] DispatchError),

    #[error(
        "A required Wayland protocol ({} version {}) is not supported by the compositor",
        name,
        version
    )]
    MissingProtocol { name: &'static str, version: u32 },

    #[error("There are no seats")]
    NoSeats,

    #[error("The clipboard of the requested seat is empty")]
    ClipboardEmpty,

    #[error("The compositor does not support primary selection")]
    PrimarySelectionUnsupported,

    #[error("The requested seat was not found")]
    SeatNotFound,

    #[error("Couldn't create a pipe for content transfer")]
    PipeCreation(#[source] io::Error),
}

pub struct Watcher {
    state: State,
    queue: EventQueue<State>,
    primary: bool,
}

pub fn initialize<S>() -> Result<(EventQueue<S>, CommonState), Error>
where
    S: Dispatch<WlRegistry, GlobalListContents> + 'static,
    S: Dispatch<ZwlrDataControlManagerV1, ()>,
    S: Dispatch<WlSeat, ()>,
    S: AsMut<CommonState>,
{
    // Connect to the Wayland compositor.
    let conn = Connection::connect_to_env().map_err(Error::WaylandConnection)?;

    // Retrieve the global interfaces.
    let (globals, queue) =
        registry_queue_init::<S>(&conn).map_err(|err| match err {
                                           GlobalError::Backend(err) => Error::WaylandCommunication(err.into()),
                                           GlobalError::InvalidId(err) => panic!("How's this possible? \
                                                                                  Is there no wl_registry? \
                                                                                  {:?}",
                                                                                 err),
                                       })?;
    let qh = &queue.handle();

    // Verify that we got the clipboard manager.
    let clipboard_manager = match globals.bind(qh, 1..=1, ()) {
        Ok(manager) => manager,
        Err(BindError::NotPresent | BindError::UnsupportedVersion) => {
            return Err(Error::MissingProtocol {
                name: ZwlrDataControlManagerV1::interface().name,
                version: 1,
            });
        }
    };

    let registry = globals.registry();
    let seats = globals.contents().with_list(|globals| {
        globals
            .iter()
            .filter(|global| global.interface == WlSeat::interface().name && global.version >= 2)
            .map(|global| {
                let seat = registry.bind(global.name, 2, qh, ());
                (seat, SeatData::default())
            })
            .collect()
    });

    let state = CommonState {
        seats,
        clipboard_manager,
    };

    Ok((queue, state))
}

impl Watcher {
    pub fn init() -> Result<Self, Error> {
        let (queue, mut common) = initialize::<State>()?;

        // Check if there are no seats.
        if common.seats.is_empty() {
            return Err(Error::NoSeats);
        }

        // Go through the seats and get their data devices.
        for (seat, data) in &mut common.seats {
            let device =
                common
                    .clipboard_manager
                    .get_data_device(seat, &queue.handle(), seat.clone());
            data.set_device(Some(device));
        }

        let state = State {
            common,
            offers: HashMap::new(),
            got_primary_selection: false,
        };

        Ok(Watcher {
            state,
            queue,
            primary: false,
        })
    }

    // note: returning an iter cause some bugs with pipes
    pub fn start_watching(&mut self, seat: Seat<'_>) -> Result<Vec<(String, Receiver)>, Error> {
        self.queue
            .blocking_dispatch(&mut self.state)
            .map_err(Error::WaylandCommunication)?;

        // Check if the compositor supports primary selection.
        if self.primary && !self.state.got_primary_selection {
            return Err(Error::PrimarySelectionUnsupported);
        }

        // Figure out which offer we're interested in.
        let data = match seat {
            Seat::Unspecified => self.state.common.seats.first().map(|e| &e.1),
            Seat::Specific(name) => self
                .state
                .common
                .seats
                .iter()
                .find(|data| data.1.name.as_deref() == Some(name))
                .map(|e| &e.1),
        };

        let Some(data) = data else {
            return Err(Error::SeatNotFound);
        };

        let offer = if self.primary {
            &data.primary_offer
        } else {
            &data.offer
        };

        // Check if we found anything.
        match offer.clone() {
            Some(offer) => {
                let mime_types = self.state.offers.remove(&offer).unwrap();

                let mut res = Vec::with_capacity(mime_types.len());

                for mime_type in mime_types {
                    // Create a pipe for content transfer.
                    let (write, read) =
                        tokio::net::unix::pipe::pipe().map_err(Error::PipeCreation)?;

                    // Start the transfer.
                    offer.receive(mime_type.clone(), write.as_fd());
                    drop(write);

                    res.push((mime_type, read));
                }

                Ok(res)
            }
            None => {
                info!("keyboard is empty");
                Err(Error::ClipboardEmpty)
            }
        }
    }
}
