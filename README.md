# A boilerplate for libp2p2

The aim of this repository is creating a boilerplate for working with [libp2p](https://github.com/libp2p/rust-libp2p) in rust.

## How to use

Clone this repository and compile it using `cargo build` command.

After compiling the code, you can run many instances as you want.
By running a new instance a greeting message will be broadcasting through the network.
You can terminate the node by pressing `Ctrl+C`

## Features

- [x] Discovering local peers using mDNS
- [ ] Discovering peers using Kademlia
- [ ] Joining the network using Bootstrap peers
- [ ] Joining the network using Bootstrap peers
- [x] Connect/Disconnect events
- [x] Sending and receiving messages using gossipsub
- [ ] Sending and receiving blocks through bitswap


## License

MIT License