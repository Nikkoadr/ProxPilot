package services

import (
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"proxmox-panel/models"
)

// ProxmoxClient handles API calls to Proxmox VE
type ProxmoxClient struct {
	BaseURL  string
	Username string
	Secret   string
	Token    string
	Client   *http.Client
}

// TerraformService manages terraform operations
type TerraformService struct {
	WorkDir string
}

// AnsibleService manages ansible operations
type AnsibleService struct {
	WorkDir string
}

// ClusterStore holds in-memory cluster data
type ClusterStore struct {
	mu       sync.RWMutex
	clusters map[string]*ClusterState
	Logs     map[string][]DeploymentLog
}

type ClusterState struct {
	Cluster  models.ProxmoxCluster
	Status   string
	Progress int
	Logs     []DeploymentLog
}

type DeploymentLog struct {
	ID        string    `json:"id"`
	ClusterID string    `json:"cluster_id"`
	Phase     string    `json:"phase"`
	Line      string    `json:"line"`
	Level     string    `json:"level"`
	Timestamp time.Time `json:"timestamp"`
}

var globalStore = &ClusterStore{
	clusters: make(map[string]*ClusterState),
	Logs:     make(map[string][]DeploymentLog),
}

func init() {
	dataDir := filepath.Join(os.TempDir(), "proxmox-panel")
	os.MkdirAll(dataDir, 0755)
}

// NewClusterStore creates a new cluster store
func NewClusterStore() *ClusterStore {
	return &ClusterStore{
		clusters: make(map[string]*ClusterState),
		Logs:     make(map[string][]DeploymentLog),
	}
}

// AddCluster stores a new cluster configuration
func (s *ClusterStore) AddCluster(cluster models.ProxmoxCluster) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.clusters[cluster.ID] = &ClusterState{
		Cluster:  cluster,
		Status:   "pending",
		Progress: 0,
		Logs:     []DeploymentLog{},
	}
}

// GetCluster retrieves a cluster by ID
func (s *ClusterStore) GetCluster(id string) (*ClusterState, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	state, ok := s.clusters[id]
	return state, ok
}

// UpdateClusterStatus updates cluster status and progress
func (s *ClusterStore) UpdateClusterStatus(id, status string, progress int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if state, ok := s.clusters[id]; ok {
		state.Status = status
		state.Progress = progress
	}
}

// AddLog adds a deployment log entry
func (s *ClusterStore) AddLog(clusterID, phase, line, level string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	log := DeploymentLog{
		ID:        fmt.Sprintf("%d", time.Now().UnixNano()),
		ClusterID: clusterID,
		Phase:     phase,
		Line:      line,
		Level:     level,
		Timestamp: time.Now(),
	}
	if state, ok := s.clusters[clusterID]; ok {
		state.Logs = append(state.Logs, log)
	}
	s.Logs[clusterID] = append(s.Logs[clusterID], log)
}

// DeleteCluster removes a cluster from the store
func (s *ClusterStore) DeleteCluster(id string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.clusters, id)
	delete(s.Logs, id)
}

// GetClusters returns all clusters
func (s *ClusterStore) GetClusters() []models.ProxmoxCluster {
	s.mu.RLock()
	defer s.mu.RUnlock()
	var clusters []models.ProxmoxCluster
	for _, state := range s.clusters {
		clusters = append(clusters, state.Cluster)
	}
	return clusters
}

// ClusterManager provides cluster lifecycle operations
type ClusterManager struct {
	store     *ClusterStore
	terraform *TerraformService
	ansible   *AnsibleService
}

// NewClusterManager creates a new cluster manager
func NewClusterManager(store *ClusterStore, terraformDir, ansibleDir string) *ClusterManager {
	return &ClusterManager{
		store:     store,
		terraform: &TerraformService{WorkDir: terraformDir},
		ansible:   &AnsibleService{WorkDir: ansibleDir},
	}
}

func (cm *ClusterManager) generateTerraformVars(cluster models.ProxmoxCluster) string {
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf(`
proxmox_api_url   = "%s"
proxmox_username  = "%s"
proxmox_password  = "%s"
target_node       = "%s"
clone_template    = "%s"
network_bridge    = "%s"
gateway           = "%s"
dns1              = "%s"
ssh_user          = "%s"
ssh_public_key    = "%s"
master_count      = %d
worker_count      = %d
master_cpu        = %d
master_ram        = %d
worker_cpu        = %d
worker_ram        = %d
`,
		cluster.ProxmoxURL,
		cluster.ProxmoxUser,
		cluster.Password,
		cluster.TargetNode,
		cluster.CloneTemplate,
		cluster.NetworkBridge,
		cluster.Gateway,
		cluster.DNS1,
		cluster.SSHUser,
		cluster.SSHPublicKey,
		cluster.MasterCount,
		cluster.WorkerCount,
		cluster.MasterCPU,
		cluster.MasterRAM,
		cluster.WorkerCPU,
		cluster.WorkerRAM,
	))
	return sb.String()
}

// ListNodes returns Proxmox nodes
func (cm *ClusterManager) ListNodes() ([]models.ProxmoxNode, error) {
	return []models.ProxmoxNode{
		{Name: "pve", Status: "online", CPU: 0.45, Memory: 67108864, UsedMem: 33554432, Disk: 53687091200, UsedDisk: 21474836480},
	}, nil
}

// ListTemplates returns available VM templates
func (cm *ClusterManager) ListTemplates() ([]models.TemplateOption, error) {
	return []models.TemplateOption{
		{Name: "ubuntu-22-04-cloudinit", Storage: "local", Size: "4GB", Description: "Ubuntu 22.04 LTS Cloud-Init"},
		{Name: "ubuntu-24-04-cloudinit", Storage: "local", Size: "4GB", Description: "Ubuntu 24.04 LTS Cloud-Init"},
		{Name: "debian-12-cloudinit", Storage: "local", Size: "3GB", Description: "Debian 12 Cloud-Init"},
	}, nil
}
