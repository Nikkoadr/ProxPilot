#!/usr/bin/env bash
# Update ProxPilot: git pull + rebuild + restart. Usage: sudo panel-update
set -euo pipefail
if [ "$(id -u)" != "0" ]; then echo "Jalankan sebagai root (sudo)."; exit 1; fi
cd /opt/proxpilot
git pull --ff-only
cd panel
export PATH=/usr/local/go/bin:$PATH GOFLAGS='-mod=mod -buildvcs=false' CGO_ENABLED=0
go build -buildvcs=false -o proxpilot .
systemctl restart proxpilot
sleep 3
curl -s -m 10 http://localhost:8080/api/health && echo && echo "Update OK"
