fn main() {
    println!("cargo:rerun-if-changed=assets/cinnabar.ico");
    println!("cargo:rerun-if-changed=app.rc");
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("Could not embed Cinnabar clicker resources");
}
