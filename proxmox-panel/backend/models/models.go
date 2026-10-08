package models

import "time"

// ProxmoxCluster represents a Proxmox cluster configuration
type ProxmoxCluster struct {
	ID              string           `json:"id"`
	Name            string           `json:"name"`
	ProxmoxURL      string           `json:"proxmox_url"`
	ProxmoxUser     string           `json:"proxmox_user"`
	ProxmoxSecret   string           `json:"proxmox_secret"`
	ProxmoxToken    string           `json:"proxmox_token,omitempty"`
	TargetNode      string           `json:"target_node"`
	CloneTemplate   string           `json:"clone_template"`
	NetworkBridge   string           `json:"network_bridge"`
	Gateway         string           `json:"gateway"`
	DNS1            string           `json:"dns1"`
	MasterCount     int              `json:"master_count"`
	WorkerCount     int              `json:"worker_count"`
	MasterCPU       int              `json:"master_cpu"`
	MasterRAM       int              `json:"master_ram"`
	WorkerCPU       int              `json:"worker_cpu"`
	WorkerRAM       int              `json:"worker_ram"`
	SSHUser         string           `json:"ssh_user"`
	SSHPublicKey    string           `json:"ssh_public_key,omitempty"`
	Password        string           `json:"password,omitempty"`
	EnabledFeatures []string         `json:"enabled_features"`
	Status          string           `json:"status"`
	CreatedAt       time.Time        `json:"created_at"`
	UpdatedAt       time.Time        `json:"updated_at"`
}

// VMConfig represents the dynamic VM configuration
type VMConfig struct {
	ClusterID string `json:"cluster_id"`
	Role      string `json:"role"` // master, worker, nginx
	Name      string `json:"name"`
	CPU       int    `json:"cpu"`
	RAM       int    `json:"ram"`
	Disk      int    `json:"disk"`
	IP        string `json:"ip"`
}

// DeploymentLog represents a log entry from terraform/ansible
type DeploymentLog struct {
	ID        string    `json:"id"`
	ClusterID string    `json:"cluster_id"`
	Phase     string    `json:"phase"` // provision, master, workers, nginx
	Line      string    `json:"line"`
	Level     string    `json:"level"` // info, warning, error
	Timestamp time.Time `json:"timestamp"`
}

// ClusterStatus represents current cluster state
type ClusterStatus struct {
	ID           string        `json:"id"`
	Name         string        `json:"name"`
	Status       string        `json:"status"` // pending, provisioning, running, error, deleted
	MasterVMs    []VMInfo      `json:"master_vms"`
	WorkerVMs    []VMInfo      `json:"worker_vms"`
	Progress     int           `json:"progress"`
	CurrentPhase string        `json:"current_phase"`
	Logs         []DeploymentLog `json:"logs"`
}

type VMInfo struct {
	Name    string `json:"name"`
	IP      string `json:"ip"`
	Status  string `json:"status"`
	Role    string `json:"role"`
	CPU     int    `json:"cpu"`
	RAM     int    `json:"ram"`
}

// ProxmoxNode represents a Proxmox node
type ProxmoxNode struct {
	Name     string `json:"name"`
	Status   string `json:"status"`
	CPU      float64 `json:"cpu"`
	Memory   uint64  `json:"memory"`
	UsedMem  uint64  `json:"used_mem"`
	Disk     uint64  `json:"disk"`
	UsedDisk uint64  `json:"used_disk"`
}

// TemplateOption represents available VM templates
type TemplateOption struct {
	Name        string `json:"name"`
	Storage     string `json:"storage"`
	Size        string `json:"size"`
	Description string `json:"description"`
}
