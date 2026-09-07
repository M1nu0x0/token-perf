use super::WebAssets;

#[test]
fn serves_module_scripts_as_javascript() {
    assert_eq!(
        super::content_type("assets/index-Gq2BWagY.js"),
        "text/javascript; charset=utf-8",
        "a wrong type here makes the browser refuse to run the bundle"
    );
    assert_eq!(
        super::content_type("index.html"),
        "text/html; charset=utf-8"
    );
    assert_eq!(super::content_type("noext"), "application/octet-stream");
}

#[test]
fn ui_is_built() {
    let index = WebAssets::get("index.html");

    assert!(
        index.is_some(),
        "web/dist is empty. Run `npm run build` in web/ first."
    );
}

#[test]
fn index_html_gets_the_configured_lang() {
    let page = r#"<html lang="ko"><body></body></html>"#;

    assert_eq!(
        super::localize(page, "en"),
        r#"<html lang="en"><body></body></html>"#
    );
    assert_eq!(super::localize(page, "ko"), page);
    // `lang` is user input: anything but a bare two-letter code is not injected.
    assert_eq!(super::localize(page, "en\" onload=\"x"), page);
    assert_eq!(super::localize(page, "EN"), page);
}
