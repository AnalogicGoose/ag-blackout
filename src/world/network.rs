use std::collections::HashMap;

use super::device::Device;

/// OS-blind device graph: identity + reachability only. Doesn't know or care
/// what OS a device runs — that's the device/OS seam's job. No routing,
/// segments or ports yet; "reachable" just means "registered."
#[derive(Default)]
pub struct Network {
    devices: HashMap<String, Device>,
}

impl Network {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, device: Device) {
        self.devices.insert(device.hostname.clone(), device);
    }

    pub fn get(&self, hostname: &str) -> Option<&Device> {
        self.devices.get(hostname)
    }

    pub fn get_mut(&mut self, hostname: &str) -> Option<&mut Device> {
        self.devices.get_mut(hostname)
    }

    pub fn is_reachable(&self, hostname: &str) -> bool {
        self.devices.contains_key(hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_device_is_reachable_and_lookup_works() {
        let mut network = Network::new();
        network.register(Device::new("web01"));
        assert!(network.is_reachable("web01"));
        assert_eq!(network.get("web01").unwrap().hostname, "web01");
    }

    #[test]
    fn unregistered_host_is_unreachable() {
        let network = Network::new();
        assert!(!network.is_reachable("nope"));
        assert!(network.get("nope").is_none());
    }
}
