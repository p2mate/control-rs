use capnpc;

fn main() {
    capnpc::CompilerCommand::new()
        .file("extron.capnp")
        .run()
        .unwrap();
}
