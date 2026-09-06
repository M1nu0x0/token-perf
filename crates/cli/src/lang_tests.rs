use super::{argv_lang, env_lang, resolve_lang, split_lang_flag};

#[test]
fn the_flag_beats_the_setting_and_the_setting_beats_the_environment() {
    assert_eq!(resolve_lang(Some("en"), Some("ko"), Some("en_US")), "en");
    assert_eq!(resolve_lang(None, Some("ko"), Some("en_US")), "ko");
    assert_eq!(resolve_lang(None, None, Some("ko_KR.UTF-8")), "ko");
    assert_eq!(resolve_lang(None, None, None), "en");
}

#[test]
fn an_unsupported_code_falls_back_to_english_without_trying_the_next_source() {
    assert_eq!(resolve_lang(None, Some("fr"), Some("ko_KR")), "en");
    assert_eq!(resolve_lang(None, None, Some("C")), "en");
    assert_eq!(resolve_lang(None, None, Some("zh_CN.UTF-8")), "en");
    assert_eq!(resolve_lang(None, None, Some("ja_JP.UTF-8")), "en");
}

#[test]
fn an_unsupported_flag_is_dropped_but_kept_for_the_warning() {
    assert_eq!(split_lang_flag(Some("fr")), (None, Some("fr")));
    assert_eq!(split_lang_flag(Some("ko")), (Some("ko"), None));
    assert_eq!(split_lang_flag(None), (None, None));
}

#[test]
fn lc_all_outranks_lang() {
    assert_eq!(
        env_lang(Some("en_US.UTF-8"), Some("ko_KR")).as_deref(),
        Some("en_US.UTF-8")
    );
    assert_eq!(env_lang(None, Some("ko_KR")).as_deref(), Some("ko_KR"));
    assert_eq!(env_lang(None, None), None);
    assert_eq!(env_lang(Some(""), Some("ko_KR")), None);
}

#[test]
fn a_locale_carries_a_region_and_an_encoding() {
    assert_eq!(resolve_lang(None, None, Some("ko_KR.UTF-8")), "ko");
    assert_eq!(resolve_lang(Some("en-US"), None, None), "en");
}

#[test]
fn the_flag_is_read_off_argv_in_both_spellings() {
    let a = |v: &[&str]| argv_lang(v.iter().map(|s| s.to_string()));
    assert_eq!(
        a(&["tp", "--lang", "ko", "sessions"]).as_deref(),
        Some("ko")
    );
    assert_eq!(a(&["tp", "sessions", "--lang=ko"]).as_deref(), Some("ko"));
    assert_eq!(a(&["tp", "sessions"]), None);
    assert_eq!(a(&["tp", "--lang"]), None);
}
