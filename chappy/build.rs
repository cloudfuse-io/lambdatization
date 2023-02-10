use tonic_build;

fn main() {
    tonic_build::compile_protos("./seed.proto").unwrap();
}
