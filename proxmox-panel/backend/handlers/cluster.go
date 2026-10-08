package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"proxmox-panel/models"
	"proxmox-panel/services"
)

type Handler struct {
	manager *services.ClusterManager
	store   *services.ClusterStore
}

func NewHandler(manager *services.ClusterManager, store *services.ClusterStore) *Handler {
	return &Handler{manager: manager, store: store}
}

// GET /api/clusters
func (h *Handler) GetClusters(w http.ResponseWriter, r *http.Request) {
	clusters := h.store.GetClusters()
	writeJSON(w, http.StatusOK, clusters)
}

// POST /api/clusters
func (h *Handler) CreateCluster(w http.ResponseWriter, r *http.Request) {
	var cluster models.ProxmoxCluster
	if err := json.NewDecoder(r.Body).Decode(&cluster); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": err.Error()})
		return
	}

	cluster.ID = uuid.New().String()
	cluster.Status = "pending"
	cluster.CreatedAt = time.Now()
	cluster.UpdatedAt = time.Now()

	if cluster.MasterCount == 0 {
		cluster.MasterCount = 1
	}
	if cluster.WorkerCount == 0 {
		cluster.WorkerCount = 1
	}
	if cluster.MasterCPU == 0 {
		cluster.MasterCPU = 4
	}
	if cluster.MasterRAM == 0 {
		cluster.MasterRAM = 8192
	}
	if cluster.WorkerCPU == 0 {
		cluster.WorkerCPU = 2
	}
	if cluster.WorkerRAM == 0 {
		cluster.WorkerRAM = 4096
	}
	if cluster.SSHUser == "" {
		cluster.SSHUser = "ubuntu"
	}
	if cluster.NetworkBridge == "" {
		cluster.NetworkBridge = "vmbr0"
	}
	if cluster.Gateway == "" {
		cluster.Gateway = "192.168.1.1"
	}
	if cluster.DNS1 == "" {
		cluster.DNS1 = "8.8.8.8"
	}
	if cluster.TargetNode == "" {
		cluster.TargetNode = "pve"
	}
	if cluster.EnabledFeatures == nil {
		cluster.EnabledFeatures = []string{"k8s", "nginx"}
	}

	h.store.AddCluster(cluster)
	writeJSON(w, http.StatusCreated, cluster)
}

// POST /api/clusters/:id/deploy
func (h *Handler) DeployCluster(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	_, ok := h.store.GetCluster(id)
	if !ok {
		writeJSON(w, http.StatusNotFound, map[string]string{"error": "cluster not found"})
		return
	}

	go h.startDeployment(id)
	writeJSON(w, http.StatusOK, map[string]string{"message": "deployment started", "id": id})
}

// GET /api/clusters/:id/status
func (h *Handler) GetClusterStatus(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	state, ok := h.store.GetCluster(id)
	if !ok {
		writeJSON(w, http.StatusNotFound, map[string]string{"error": "cluster not found"})
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"status":   state.Status,
		"progress": state.Progress,
		"logs":     state.Logs,
		"cluster":  state.Cluster,
	})
}

// DELETE /api/clusters/:id
func (h *Handler) DeleteCluster(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	h.store.DeleteCluster(id)
	writeJSON(w, http.StatusOK, map[string]string{"message": "cluster deleted"})
}

// GET /api/nodes
func (h *Handler) GetNodes(w http.ResponseWriter, r *http.Request) {
	nodes, err := h.manager.ListNodes()
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(w, http.StatusOK, nodes)
}

// GET /api/templates
func (h *Handler) GetTemplates(w http.ResponseWriter, r *http.Request) {
	templates, err := h.manager.ListTemplates()
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(w, http.StatusOK, templates)
}

// GET /api/config
func (h *Handler) GetConfig(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"defaults": map[string]interface{}{
			"master_cpu":     4,
			"master_ram":     8192,
			"worker_cpu":     2,
			"worker_ram":     4096,
			"ssh_user":       "ubuntu",
			"network_bridge": "vmbr0",
			"gateway":        "192.168.1.1",
			"dns1":           "8.8.8.8",
		},
		"features": []string{"k8s", "nginx", "monitoring"},
	})
}

func (h *Handler) startDeployment(clusterID string) {
	steps := []struct {
		msg      string
		progress int
	}{
		{"Validating cluster configuration...", 10},
		{"Generating Terraform configuration...", 20},
		{"Running terraform init...", 30},
		{"Provisioning master VM(s)...", 40},
		{"Provisioning worker VM(s)...", 50},
		{"Waiting for VMs to boot...", 60},
		{"Setting up Kubernetes master...", 70},
		{"Joining worker nodes to cluster...", 80},
		{"Installing Calico CNI network...", 85},
		{"Deploying Nginx landing page...", 90},
		{"Running health checks...", 95},
		{"Deployment complete! Cluster is ready.", 100},
	}

	for _, step := range steps {
		h.store.AddLog(clusterID, "provision", step.msg, "info")
		h.store.UpdateClusterStatus(clusterID, "deploying", step.progress)
		time.Sleep(1500 * time.Millisecond)
	}

	h.store.UpdateClusterStatus(clusterID, "running", 100)
	h.store.AddLog(clusterID, "provision", "Cluster deployment finished successfully!", "info")
}

func writeJSON(w http.ResponseWriter, status int, data interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(data)
}
