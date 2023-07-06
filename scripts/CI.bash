git pull
cargo build --release
sudo systemctl stop fip-voting-server.service
sudo cp target/release/fip-voting /usr/bin/fip-voting
sudo setcap 'cap_net_bind_service=+ep' /usr/bin/fip-voting
sudo systemctl start fip-voting-server.service
