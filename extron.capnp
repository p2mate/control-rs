@0xdba848f2af5224e5;

interface ControlExtron {
    struct ExtronDevice {
        name @0 :Text;
        path @1 :Text;
    }

    listDevices @0 () -> (reply: List(ExtronDevice));
    selectInput @1 (name: Text, input: Text);
    rescan @2 ();
    stopServer @3 ();
}