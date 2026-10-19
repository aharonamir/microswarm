use ms_agent::exit::reason_for_code;
use ms_proto::ExitReason;

#[test]
fn known_exit_codes_map_to_contract_reasons_and_unknown_codes_fail_closed() {
    assert_eq!(reason_for_code(0), ExitReason::Ok);
    assert_eq!(reason_for_code(75), ExitReason::Transient);
    assert_eq!(reason_for_code(77), ExitReason::Safety);
    assert_eq!(reason_for_code(78), ExitReason::Config);
    assert_eq!(reason_for_code(99), ExitReason::Safety);
}
