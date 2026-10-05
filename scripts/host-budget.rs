use mod_host::{ControlEvent, ModGrants, ModHost, empty_controls};

#[test]
fn saved_preferences_and_panel_updates_fit_the_real_wasm_callback_budget() {
    let component =
        std::fs::read(std::env::var_os("CINNAROIDS_BUDGET_COMPONENT").unwrap()).unwrap();
    for settings in [
        "{}",
        r#"{"cps":20,"toggle_key":"VK:119","aim_strength":100,"aim_mode":"continuous","dark_mode":true}"#,
        r#"{"cps":30,"clicker_key":"NumpadSubtract","aim_key":"ArrowRight","reach_key":"NumpadDecimal","aim_strength":100,"aim_mode":"continuous","reach_blocks":6,"dark_mode":false,"fake_lag_ms":1000,"show_real_position":true,"netherite_range":128}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("module.wasm");
        std::fs::write(path.with_extension("settings.json"), settings).unwrap();
        let mut host = ModHost::load_snapshot_with_grants(
            &path,
            &component,
            ModGrants {
                players: true,
                camera: true,
                controls: true,
                interaction: true,
                settings: true,
                packet_delay: true,
                block_highlights: true,
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("init settings {settings}: {error:#}"));
        for (id, value) in [
            ("netherite", 1.0),
            ("netherite_range", 128.0),
            ("dark_mode", 1.0),
            ("aim", 1.0),
            ("clicker", 1.0),
            ("stop_all", 1.0),
        ] {
            let mut controls = empty_controls();
            controls.focused = true;
            controls.panel_open = true;
            controls.events.push(ControlEvent {
                id: id.into(),
                value,
            });
            host.frame_with_controls(false, None, controls)
                .unwrap_or_else(|error| panic!("event {id}, settings {settings}: {error:#}"));
            assert!(host.is_active());
            assert!(host.panel().is_some());
        }
        assert!(host.block_highlights().is_none());
    }
}
