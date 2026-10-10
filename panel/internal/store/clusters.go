package store

import (
	"encoding/json"
	"fmt"
	"time"
)

// VMSpec is one VM with its own sizing/network.
type VMSpec struct {
	Name   string `json:"name"`
	CPU    int    `json:"cpu"`
	RAM    int    `json:"ram"`
	DiskGB int    `json:"disk_gb"`
	Bridge string `json:"bridge"`
	IPMode string `json:"ip_mode"`
	IP     string `json:"ip"`
}

// Cluster is a terraform-managed group of VMs.
type Cluster struct {
	ID           string   `json:"id"`
	Name         string   `json:"name"`
	Node         string   `json:"node"`
	TemplateVMID int      `json:"template_vmid"`
	TemplateName string   `json:"template_name"`
	Masters      int      `json:"masters"`
	Workers      int      `json:"workers"`
	VMNames      []string `json:"vm_names"`
	VMSpecs      []VMSpec `json:"vm_specs"`
	CPU          int      `json:"cpu"`
	RAM          int      `json:"ram"`
	DiskGB       int      `json:"disk_gb"`
	Bridge       string   `json:"bridge"`
	IPMode       string   `json:"ip_mode"`
	BaseIP       string   `json:"base_ip"`
	Status       string   `json:"status"`
	CreatedAt    int64    `json:"created_at"`
}

// VMList returns explicit names, or legacy masters/workers fallback.
func (c Cluster) VMList() []string {
	if len(c.VMSpecs) > 0 {
		out := make([]string, 0, len(c.VMSpecs))
		for _, s := range c.VMSpecs {
			out = append(out, s.Name)
		}
		return out
	}
	if len(c.VMNames) > 0 {
		return c.VMNames
	}
	var out []string
	for i := 0; i < c.Masters; i++ {
		out = append(out, fmt.Sprintf("%s-master-%d", c.Name, i))
	}
	for i := 0; i < c.Workers; i++ {
		out = append(out, fmt.Sprintf("%s-worker-%d", c.Name, i))
	}
	return out
}

// Host is one deployed VM with known address.
type Host struct {
	ClusterID string `json:"cluster_id"`
	VMName    string `json:"vm_name"`
	VMID      int    `json:"vmid"`
	IP        string `json:"ip"`
}

func NewID() string {
	return fmt.Sprintf("c%d", time.Now().UnixNano()/1000)
}

func (s *Store) CreateCluster(c Cluster) error {
	c.CreatedAt = time.Now().Unix()
	if c.Status == "" {
		c.Status = "pending"
	}
	names, _ := json.Marshal(c.VMNames)
	specs, _ := json.Marshal(c.VMSpecs)
	_, err := s.db.Exec(`INSERT INTO clusters(id,name,node,template_vmid,template_name,masters,workers,vm_names,vm_specs,cpu,ram,disk_gb,bridge,ip_mode,base_ip,status,created_at)
		VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`, c.ID, c.Name, c.Node, c.TemplateVMID, c.TemplateName,
		c.Masters, c.Workers, string(names), string(specs), c.CPU, c.RAM, c.DiskGB, c.Bridge, c.IPMode, c.BaseIP, c.Status, c.CreatedAt)
	return err
}

func (s *Store) ListClusters() ([]Cluster, error) {
	rows, err := s.db.Query(`SELECT id,name,node,template_vmid,template_name,masters,workers,vm_names,vm_specs,cpu,ram,disk_gb,bridge,ip_mode,base_ip,status,created_at FROM clusters ORDER BY created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Cluster
	for rows.Next() {
		var c Cluster
		var names, specs string
		if err := rows.Scan(&c.ID, &c.Name, &c.Node, &c.TemplateVMID, &c.TemplateName, &c.Masters, &c.Workers,
			&names, &specs, &c.CPU, &c.RAM, &c.DiskGB, &c.Bridge, &c.IPMode, &c.BaseIP, &c.Status, &c.CreatedAt); err != nil {
			return nil, err
		}
		_ = json.Unmarshal([]byte(names), &c.VMNames)
		_ = json.Unmarshal([]byte(specs), &c.VMSpecs)
		out = append(out, c)
	}
	return out, nil
}

func (s *Store) GetCluster(id string) (Cluster, bool) {
	var c Cluster
	var names, specs string
	err := s.db.QueryRow(`SELECT id,name,node,template_vmid,template_name,masters,workers,vm_names,vm_specs,cpu,ram,disk_gb,bridge,ip_mode,base_ip,status,created_at FROM clusters WHERE id=?`, id).
		Scan(&c.ID, &c.Name, &c.Node, &c.TemplateVMID, &c.TemplateName, &c.Masters, &c.Workers,
			&names, &specs, &c.CPU, &c.RAM, &c.DiskGB, &c.Bridge, &c.IPMode, &c.BaseIP, &c.Status, &c.CreatedAt)
	if err != nil {
		return c, false
	}
	_ = json.Unmarshal([]byte(names), &c.VMNames)
	_ = json.Unmarshal([]byte(specs), &c.VMSpecs)
	return c, true
}

func (s *Store) SetClusterStatus(id, status string) {
	_, _ = s.db.Exec(`UPDATE clusters SET status=? WHERE id=?`, status, id)
}

func (s *Store) DeleteCluster(id string) {
	_, _ = s.db.Exec(`DELETE FROM cluster_hosts WHERE cluster_id=?`, id)
	_, _ = s.db.Exec(`DELETE FROM clusters WHERE id=?`, id)
}

func (s *Store) SaveHosts(clusterID string, hosts []Host) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM cluster_hosts WHERE cluster_id=?`, clusterID); err != nil {
		tx.Rollback()
		return err
	}
	for _, h := range hosts {
		if _, err := tx.Exec(`INSERT INTO cluster_hosts(cluster_id,vm_name,vmid,ip) VALUES(?,?,?,?)`,
			clusterID, h.VMName, h.VMID, h.IP); err != nil {
			tx.Rollback()
			return err
		}
	}
	return tx.Commit()
}

func (s *Store) ClusterHosts(clusterID string) ([]Host, error) {
	rows, err := s.db.Query(`SELECT cluster_id,vm_name,vmid,ip FROM cluster_hosts WHERE cluster_id=? ORDER BY vm_name`, clusterID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []Host
	for rows.Next() {
		var h Host
		if err := rows.Scan(&h.ClusterID, &h.VMName, &h.VMID, &h.IP); err != nil {
			return nil, err
		}
		out = append(out, h)
	}
	return out, nil
}
