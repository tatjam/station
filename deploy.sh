cargo build --release --target x86_64-unknown-linux-musl
scp target/x86_64-unknown-linux-musl/release/station tatjam@ssh-tatjam.alwaysdata.net:/home/tatjam/station/station
