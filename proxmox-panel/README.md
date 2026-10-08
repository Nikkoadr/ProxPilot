# Proxmox Panel - Kubernetes Cluster Deployer

Aplikasi web untuk otomatisasi provisioning Kubernetes cluster di Proxmox VE menggunakan Terraform dan Ansible.

**Tech Stack:** Go (backend) + React/Vite (frontend) + Terraform + Ansible

---

## Requirements

- **Go** 1.22+
- **Node.js** 18+
- **Terraform** (opsional, untuk deployment real)
- **Ansible** (opsional, untuk konfigurasi VM)
- **Proxmox VE** dengan cloud-init template

---

## Quick Start (Satu Klik)

### Windows

Klik dua kali file berikut:

```
start.bat
```

Atau jalankan via terminal:

```bash
cd proxmox-panel
.\start.bat
```

Setelah server berjalan, buka: **http://localhost:8080**

---

### Manual

#### 1. Install Dependencies

```bash
# Frontend
cd frontend
npm install

# Backend
cd ../backend
go mod tidy
```

#### 2. Build Frontend

```bash
cd ../frontend
npm run build
```

#### 3. Jalankan Backend

```bash
cd ../backend
go run main.go
```

Server berjalan di **http://localhost:8080**

---

## Development Mode

Untuk development dengan hot-reload:

```bash
.\start-dev.bat
```

- Backend: http://localhost:8080
- Frontend: http://localhost:5173

---

## Menggunakan Make (Opsional)

```bash
# Install semua deps
make install

# Build frontend
make build

# Jalankan server
make run

# Clean
make clean
```

---

## API Endpoints

| Method | Endpoint | Deskripsi |
|--------|----------|-----------|
| GET | `/api/clusters` | Daftar semua cluster |
| POST | `/api/clusters` | Buat cluster baru |
| POST | `/api/clusters/:id/deploy` | Deploy cluster |
| GET | `/api/clusters/:id/status` | Status & log cluster |
| DELETE | `/api/clusters/:id` | Hapus cluster |
| GET | `/api/nodes` | Daftar node Proxmox |
| GET | `/api/templates` | Template VM tersedia |
| GET | `/api/config` | Default config & fitur |
| GET | `/api/ws/logs/:id` | WebSocket real-time log |

---

## Struktur Project

```
proxmox-panel/
├── backend/              # Go backend
│   ├── main.go           # Entry point server
│   ├── go.mod
│   ├── models/           # Data structures
│   ├── services/         # Business logic
│   └── handlers/         # HTTP handlers
├── frontend/             # React + Vite
│   ├── src/
│   │   ├── components/   # UI components
│   │   ├── pages/        # Dashboard, NewCluster, ClusterDetail
│   │   ├── context/      # React context (cluster state)
│   │   └── api/          # API client
│   ├── dist/             # Production build
│   └── node_modules/
├── infra/
│   ├── terraform/        # Generated Terraform configs
│   └── ansible/          # Ansible playbooks
├── start.bat             # One-click start (Windows)
├── start-dev.bat         # Development mode
└── Makefile
```

---

## Flow Deployment

1. User isi form → **Create Cluster**
2. Backend simpan config → status `pending`
3. User klik **Deploy** → status `provisioning`
4. Backend generate Terraform config → jalankan `terraform apply`
5. VM Proxmox provisioned → status `deploying`
6. Backend jalankan Ansible playbook → install K8s + Nginx
7. Deployment selesai → status `running`
8. User akses landing page di IP master node

---

## Troubleshooting

**Backend tidak bisa jalan:**
```bash
cd backend
go clean -modcache
go mod tidy
go run main.go
```

**Frontend error:**
```bash
cd frontend
rm -rf node_modules dist
npm install
npm run build
```

**Port sudah dipakai:**
```bash
# Ubah port di backend/main.go atau set env
set PORT=9090
go run main.go
```

---

## License

Private Project
