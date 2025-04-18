# ptunp - Peer to Peer TUN network interface

> [!NOTE] THIS CODE IS WIP AND UNTESTED

Peer to peer vpn over [iroh][iroh] (p2p QUIC). It uses [tun][tun] network interfaces
which are pseudo devices that allow applications to send OSI layer 3 network packets
over an application defined transport.

Note: root is required on linux.

## License

Unless otherwise specified, all code in this repository is dual-licensed under
either:

- MIT-0 License ([LICENSE-MIT-0](LICENSE-MIT-0))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option. This means you can select the license you prefer!

Any contribution intentionally submitted for inclusion in the work by you, shall be
dual licensed as above, without any additional terms or conditions.

[iroh]: https://www.iroh.computer/ 
[tun]: https://github.com/meh/rust-tun
