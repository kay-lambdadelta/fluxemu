use std::{
    fs::{File, OpenOptions},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    path::{Path, PathBuf},
};

use drm::{
    ClientCapability, Device as _,
    control::{Device, Mode, ModeTypeFlags, PlaneType},
};
use libseat::Seat;
use udev::Enumerator;

#[derive(Debug)]
pub struct Card(File);

impl Card {
    pub fn find_suitable_card(seat: &mut Seat) -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("DRM_DEVICE_PATH") {
            return Some(PathBuf::from(path));
        }

        let mut enumerator = Enumerator::new().expect("Creating udev enumerator");
        enumerator
            .match_subsystem("drm")
            .expect("Matching drm subsystem");

        // Filter for an appropriate DRM device
        enumerator
            .scan_devices()
            .expect("Scanning udev devices")
            .filter(|device| {
                device
                    .devnode()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("card"))
                    .unwrap_or(false)
            })
            .find(|device| {
                device
                    .property_value("ID_SEAT")
                    .and_then(|name| name.to_str())
                    .unwrap_or("seat0")
                    == seat.name()
            })
            .and_then(|device| device.devnode().map(PathBuf::from))
    }

    pub fn open(card_path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        tracing::info!("Opening DRM card at {:?}", card_path.as_ref());

        let me = Self(OpenOptions::new().read(true).write(true).open(card_path)?);
        me.set_client_capability(ClientCapability::UniversalPlanes, true)?;

        Ok(me)
    }

    pub fn select_suitable_params(&self) -> DrmParams {
        let resources = self.resource_handles().expect("Querying DRM resources");

        let (connector_info, mode) = resources
            .connectors()
            .iter()
            .map(|handle| {
                self.get_connector(*handle, true)
                    .expect("Querying connector")
            })
            .find(|info| {
                info.state() == drm::control::connector::State::Connected
                    && !info.modes().is_empty()
            })
            .map(|info| {
                // Find the preferred mode
                let mode = info
                    .modes()
                    .iter()
                    .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
                    .copied()
                    .unwrap_or(info.modes()[0]);

                (info, mode)
            })
            .expect("No connected connector with modes found");

        let crtc_handle = connector_info
            .current_encoder()
            .and_then(|handle| self.get_encoder(handle).ok().and_then(|info| info.crtc()))
            .or_else(|| {
                connector_info
                    .encoders()
                    .iter()
                    .filter_map(|handle| self.get_encoder(*handle).ok())
                    .find_map(|info| {
                        resources
                            .filter_crtcs(info.possible_crtcs())
                            .into_iter()
                            .next()
                    })
            })
            .expect("No CRTC available for connector");

        DrmParams {
            connector_handle: connector_info.handle(),
            crtc_handle,
            mode,
        }
    }

    pub fn find_suitable_plane(
        &self,
        crtc_id: drm::control::crtc::Handle,
    ) -> drm::control::plane::Handle {
        let resources = self.resource_handles().expect("Querying DRM resources");
        let planes = self.plane_handles().expect("Querying plane handles");

        planes
            .into_iter()
            .filter(|&handle| {
                self.get_plane(handle)
                    .map(|info| {
                        resources
                            .filter_crtcs(info.possible_crtcs())
                            .contains(&crtc_id)
                    })
                    .unwrap_or(false)
            })
            .find(|&handle| {
                let Ok(props) = self.get_properties(handle) else {
                    return false;
                };

                let (prop_handles, prop_values) = props.as_props_and_values();

                prop_handles
                    .iter()
                    .zip(prop_values.iter())
                    .any(|(&handle, &plane_type)| {
                        self.get_property(handle)
                            .ok()
                            .and_then(|info| info.name().to_str().ok().map(|n| n == "type"))
                            .unwrap_or(false)
                            && plane_type == PlaneType::Primary as u64
                    })
            })
            .expect("No suitable plane found")
    }
}

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for Card {
    fn as_raw_fd(&self) -> i32 {
        self.0.as_raw_fd()
    }
}

impl drm::Device for Card {}
impl drm::control::Device for Card {}

#[derive(Debug)]
pub struct DrmParams {
    pub connector_handle: drm::control::connector::Handle,
    pub crtc_handle: drm::control::crtc::Handle,
    pub mode: Mode,
}
