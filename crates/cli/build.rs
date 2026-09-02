fn main() {
    // `i18n!` reads locales/ at macro expansion, which cargo cannot see — without
    // this, editing a translation silently rebuilds nothing.
    println!("cargo::rerun-if-changed=locales");
}
