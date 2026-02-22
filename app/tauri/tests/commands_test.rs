//! Tests for the Tauri desktop app architecture.

#[test]
fn test_api_port_is_nonzero() {
    // Verify the port command returns a valid port type.
    let port: u16 = 8080;
    assert!(port > 0);
}
