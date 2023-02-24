use tonic_build;

fn main() {
    tonic_build::configure()
        .type_attribute("seed.Address", "#[derive(Eq, Hash)]")
        .compile(&["./seed.proto"], &[] as &[String])
        .unwrap();
}
