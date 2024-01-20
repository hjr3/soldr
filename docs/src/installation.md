# Installation

Soldr is not currently pre-packaged.

To install:

- `git clone https://github.com/hjr3/soldr.git`
- `cd soldr`
- `just install build-ui`
   - If you do not have `just` installed, you can manually run the commands from the file.
- `cargo build --release` will create two targets:
   - `target/release/soldr` - this is the proxy and the management API
   - `target/release/ui` - this a file server that serves the management UI. The management UI is optional.
