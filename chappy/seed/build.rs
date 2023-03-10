use tonic_build;

fn main() {
    tonic_build::configure()
        .compile(&["seed.proto"], &["."])
        .unwrap();
}
