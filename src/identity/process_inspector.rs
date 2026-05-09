use crate::process::{resolve_process};
use crate::media_source::ProcessIdentity;

#[derive(Clone, Debug, Default)]
pub struct ProcessInspector;

impl ProcessInspector {
    pub fn new() -> Self {
        Self
    }

    pub fn inspect_process(&self, process_id: u32) -> ProcessIdentity {
        let snapshot = resolve_process(process_id, "Unknown".to_string());
        ProcessIdentity {
            process_id: snapshot.process_id,
            creation_time: snapshot.creation_time,
            executable_path: snapshot.executable_path,
            executable_name: snapshot.executable_name,
            package_full_name: snapshot.package_full_name,
        }
    }
}
