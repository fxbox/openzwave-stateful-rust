extern crate openzwave;
use openzwave::{options, manager, controller};
use openzwave::notification::*;
use openzwave::node::*;
pub use openzwave::value_classes::value_id::{ ValueID, ValueGenre };
use std::fs;
use std::sync::{ Arc, Mutex };
use std::collections::{ BTreeSet, HashMap, HashSet };
use std::sync::MutexGuard;

#[cfg(windows)]
fn get_default_device() {
    "\\\\.\\COM6"
}

#[cfg(unix)]
fn get_default_device() -> Option<&'static str> {
    let default_devices = [
        "/dev/cu.usbserial", // MacOS X
        "/dev/cu.SLAB_USBtoUART", // MacOS X
        "/dev/ttyUSB0", // Linux
        "/dev/ttyACM0"  // Linux (Aeotech Z-Stick Gen-5)
    ];

    default_devices
        .iter()
        .find(|device_name| fs::metadata(device_name).is_ok())
        .map(|&str| str)
}

#[derive(Debug, Clone)]
pub struct State {
    controllers: HashSet<controller::Controller>,
    nodes: BTreeSet<Node>,
    nodes_map: HashMap<controller::Controller, BTreeSet<Node>>,
    value_ids: BTreeSet<ValueID>,
}

impl State {
    fn new() -> State {
        State {
            controllers: HashSet::new(),
            nodes: BTreeSet::new(),
            nodes_map: HashMap::new(),
            value_ids: BTreeSet::new()
        }
    }

    pub fn get_controllers(&self) -> &HashSet<controller::Controller> {
        &self.controllers
    }

    pub fn get_nodes(&self) -> &BTreeSet<Node> {
        &self.nodes
    }

    pub fn get_nodes_map(&self) -> &HashMap<controller::Controller, BTreeSet<Node>> {
        &self.nodes_map
    }

    pub fn get_values(&self) -> &BTreeSet<ValueID> {
        &self.value_ids
    }

    pub fn add_node(&mut self, node: Node) {
        let node_set = self.nodes_map.entry(node.get_controller()).or_insert(BTreeSet::new());
        node_set.insert(node);
        self.nodes.insert(node);
    }

    pub fn remove_node(&mut self, node: Node) {
        if let Some(node_set) = self.nodes_map.get_mut(&node.get_controller()) {
            node_set.remove(&node);
        }
        self.nodes.remove(&node);
    }

    pub fn add_value_id(&mut self, value_id: ValueID) {
        self.value_ids.insert(value_id.clone());
        println!("Added value_id: {:?}", value_id);
    }

    pub fn remove_value_id(&mut self, value_id: ValueID) {
        self.value_ids.remove(&value_id);
    }
}

pub struct ZWaveManager {
    watcher: ZWaveWatcher,
    ozw_manager: manager::Manager
}

impl ZWaveManager {
    fn new(manager: manager::Manager) -> ZWaveManager {
        ZWaveManager {
            watcher: ZWaveWatcher {
                state: Arc::new(Mutex::new(State::new())),
            },
            ozw_manager: manager
        }
    }

    fn add_watcher(&mut self) -> Result<(), ()> {
        self.ozw_manager.add_watcher(self.watcher.clone()).and(Ok(()))
    }

    fn add_driver(&mut self, device: &str) -> Result<(), ()> {
        match device {
            "usb" => self.ozw_manager.add_usb_driver(),
            _ => self.ozw_manager.add_driver(&device)
        }
    }

    pub fn get_state(&self) -> MutexGuard<State> {
        self.watcher.get_state()
    }
}

#[derive(Clone)]
struct ZWaveWatcher {
    state: Arc<Mutex<State>>
}

impl ZWaveWatcher {
    pub fn get_state(&self) -> MutexGuard<State> {
        self.state.lock().unwrap()
    }
}

impl manager::NotificationWatcher for ZWaveWatcher {
    fn on_notification(&self, notification: Notification) {
        //println!("Received notification: {:?}", notification);

        match notification.get_type() {
            NotificationType::Type_DriverReady => {
                let controller = notification.get_controller();
                let mut state = self.get_state();
                if !state.controllers.contains(&controller) {
                    println!("Found new controller: {:?}", controller);
                    state.controllers.insert(controller);
                }
            },
            NotificationType::Type_NodeAdded => {
                let mut state = self.get_state();
                let node = notification.get_node();
                println!("NodeAdded: {:?}", node);
                state.add_node(node);
            },
            NotificationType::Type_NodeRemoved => {
                let mut state = self.get_state();
                let node = notification.get_node();
                println!("NodeRemoved: {:?}", node);
                state.remove_node(node);
            },
            NotificationType::Type_NodeEvent => {
                println!("NodeEvent");
            },
            NotificationType::Type_ValueAdded => {
                let mut state = self.get_state();
                let value_id = notification.get_value_id();
                println!("ValueAdded: {:?}", value_id);
                state.add_value_id(value_id);
            },
            NotificationType::Type_ValueChanged => {
                let mut state = self.get_state();
                let value_id = notification.get_value_id();
                println!("ValueChanged: {:?}", value_id);
                state.add_value_id(value_id);
                // TODO: Tell somebody that the value changed
            },
            NotificationType::Type_ValueRemoved => {
                let mut state = self.get_state();
                let value_id = notification.get_value_id();
                println!("ValueRemoved: {:?}", value_id);
                state.remove_value_id(value_id);
            },
            _ => {
                //println!("Unknown notification: {:?}", notification);
            }
        }
    }
}

pub struct InitOptions {
    pub device: Option<String>
}

pub fn init(options: &InitOptions) -> Result<ZWaveManager,()> {
    let mut ozw_options = try!(options::Options::create("./config/", "", "--SaveConfiguration true --DumpTriggerLevel 0 --ConsoleOutput false"));

    try!(options::Options::add_option_int(&mut options, "PollInterval", 60000)); //enable polling each 60 seconds
    try!(options::Options::add_option_bool(&mut options, "IntervalBetweenPolls", true));
    // TODO: The NetworkKey should really be derived from something unique
    //       about the foxbox that we're running on. This particular set of
    //       values happens to be the default that domoticz uses.
    try!(options::Options::add_option_string(&mut ozw_options, "NetworkKey", "0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10", false));

    let manager = try!(manager::Manager::create(ozw_options));
    let mut zwave_manager = ZWaveManager::new(manager);
    try!(zwave_manager.add_watcher());

    let device = match options.device {
        Some(ref device) => device as &str,
        _ => try!(get_default_device().ok_or(()))
    };

    //println!("found device {}", device);

    try!(zwave_manager.add_driver(&device));

    Ok(zwave_manager)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
