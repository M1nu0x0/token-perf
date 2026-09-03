use token_perf_core::store::Store;

const LANGS: [&str; 3] = ["en", "ko", "ja"];

pub fn supported(code: &str) -> Option<&'static str> {
    let short = code.split(['_', '.', '-']).next().unwrap_or_default();
    LANGS.into_iter().find(|l| *l == short)
}

pub fn resolve_lang(flag: Option<&str>, saved: Option<&str>, env: Option<&str>) -> &'static str {
    flag.or(saved).or(env).and_then(supported).unwrap_or("en")
}

/// POSIX: LC_ALL overrides LANG.
pub fn env_lang(lc_all: Option<&str>, lang: Option<&str>) -> Option<String> {
    lc_all
        .or(lang)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub fn process_lang() -> Option<String> {
    env_lang(
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

/// Set twice so a store failure still reports in a sensible language.
pub fn apply_lang(store: &Store, flag: Option<&str>) -> String {
    let saved = store.setting("lang").ok().flatten();
    let lang = resolve_lang(flag, saved.as_deref(), process_lang().as_deref());
    if flag.is_some() {
        store.set_setting("lang", lang).ok();
    }
    rust_i18n::set_locale(lang);
    lang.to_string()
}

/// A typo must not erase a stored preference: an unknown code is dropped, not saved.
pub fn split_lang_flag(flag: Option<&str>) -> (Option<&str>, Option<&str>) {
    match flag {
        Some(code) if supported(code).is_none() => (None, Some(code)),
        code => (code, None),
    }
}

#[cfg(test)]
#[path = "lang_tests.rs"]
mod tests;
