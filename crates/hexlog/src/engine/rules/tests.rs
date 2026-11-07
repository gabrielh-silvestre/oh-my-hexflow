use super::instant_is_well_formed;

#[test]
fn it_should_accept_only_the_24_character_utc_instant_shape() {
    let accepted = ["2026-11-07T12:00:00.000Z"];
    let refused = [
        "2026-11-07T12:00:00.00Z",
        "2026-11-07T12:00:00.0000Z",
        "2026-11-07T12:00:00.0000",
        "2026-11-07 12:00:00.000Z",
        "2026-1l-07T12:00:00.000Z",
        "2026-11-07T12:00:0o.000Z",
        "",
    ];

    for text in accepted {
        assert!(instant_is_well_formed(text), "should accept {text:?}");
    }
    for text in refused {
        assert!(!instant_is_well_formed(text), "should refuse {text:?}");
    }
}
