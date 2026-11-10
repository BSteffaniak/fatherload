fn main() {
    // This JavaScript is linked by scripts/build-web.sh, so Cargo must relink after edits.
    println!("cargo:rerun-if-changed=../../web/bootstrap.js");
}
