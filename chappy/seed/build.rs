fn main() {
    tonic_build::configure()
        .compile(&["seed.proto"], &["."])
        .unwrap();
}
