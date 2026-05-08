use audiofocus::{
    events::{AudioSessionSnapshot, AudioSessionStateKind},
    registry::AudioSessionRegistry,
};

fn snapshot(process_id: u32, state: AudioSessionStateKind) -> AudioSessionSnapshot {
    AudioSessionSnapshot {
        process_id,
        display_name: format!("process-{process_id}"),
        state,
        peak: 0.25,
        session_count: 1,
    }
}

#[test]
fn emits_start_and_active_for_new_active_process() {
    let mut registry = AudioSessionRegistry::default();
    let events = registry.reconcile(vec![snapshot(42, AudioSessionStateKind::Active)]);

    let names = events.iter().map(|event| event.name()).collect::<Vec<_>>();
    assert_eq!(names, vec!["SessionStarted", "SessionBecameActive"]);
}

#[test]
fn suppresses_stop_during_short_session_recreation_gap() {
    let mut registry = AudioSessionRegistry::default();
    registry.reconcile(vec![snapshot(42, AudioSessionStateKind::Active)]);

    let first_missing = registry.reconcile(Vec::new());
    let recreated = registry.reconcile(vec![snapshot(42, AudioSessionStateKind::Active)]);

    assert!(first_missing.is_empty());
    assert!(recreated.is_empty());
}

#[test]
fn emits_stop_after_confirmed_disappearance() {
    let mut registry = AudioSessionRegistry::default();
    registry.reconcile(vec![snapshot(42, AudioSessionStateKind::Inactive)]);

    assert!(registry.reconcile(Vec::new()).is_empty());
    assert!(registry.reconcile(Vec::new()).is_empty());

    let events = registry.reconcile(Vec::new());
    let names = events.iter().map(|event| event.name()).collect::<Vec<_>>();
    assert_eq!(names, vec!["SessionStopped"]);
}
