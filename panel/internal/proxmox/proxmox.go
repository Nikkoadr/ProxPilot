package proxmox

import (
	"bytes"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"time"
)

// Creds authenticates via PVEAPIToken header.
type Creds struct {
	BaseURL     string
	User        string
	TokenID     string
	TokenSecret string
	VerifyTLS   bool
}

type Node struct {
	Name   string  `json:"name"`
	Status string  `json:"status"`
	CPU    float64 `json:"cpu"`
	MaxMem uint64  `json:"maxmem"`
	Mem    uint64  `json:"mem"`
	Live   bool    `json:"live"`
}

type VM struct {
	VMID     uint64  `json:"vmid"`
	Name     string  `json:"name"`
	Status   string  `json:"status"`
	CPUs     uint32  `json:"cpus"`
	CPU      float64 `json:"cpu"`
	Mem      uint64  `json:"mem"`
	MaxMem   uint64  `json:"maxmem"`
	Uptime   uint64  `json:"uptime"`
	Template bool    `json:"template"`
	Node     string  `json:"node,omitempty"`
}

func client(verifyTLS bool) *http.Client {
	return &http.Client{
		Timeout: 15 * time.Second,
		Transport: &http.Transport{
			TLSClientConfig: &tls.Config{InsecureSkipVerify: !verifyTLS},
		},
	}
}

func apiURL(base, path string) string {
	b := strings.TrimSuffix(base, "/")
	if strings.HasSuffix(b, "/api2/json") {
		return b + path
	}
	return b + "/api2/json" + path
}

func (c *Creds) auth() string {
	return fmt.Sprintf("PVEAPIToken=%s!%s=%s", c.User, c.TokenID, c.TokenSecret)
}

func (c *Creds) get(path string, query map[string]string) (map[string]any, int, error) {
	u := apiURL(c.BaseURL, path)
	if len(query) > 0 {
		q := url.Values{}
		for k, v := range query {
			q.Set(k, v)
		}
		u += "?" + q.Encode()
	}
	req, _ := http.NewRequest("GET", u, nil)
	req.Header.Set("Authorization", c.auth())
	resp, err := client(c.VerifyTLS).Do(req)
	if err != nil {
		return nil, 0, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	var v map[string]any
	if err := json.Unmarshal(body, &v); err != nil {
		return nil, resp.StatusCode, fmt.Errorf("bad json: %s", limitStr(string(body), 200))
	}
	return v, resp.StatusCode, nil
}

func (c *Creds) post(path string, payload any) (map[string]any, int, error) {
	return c.write("POST", path, payload)
}

func (c *Creds) put(path string, payload any) (map[string]any, int, error) {
	return c.write("PUT", path, payload)
}

func (c *Creds) del(path, rawQuery string) (int, string, error) {
	u := apiURL(c.BaseURL, path) + rawQuery
	req, _ := http.NewRequest("DELETE", u, nil)
	req.Header.Set("Authorization", c.auth())
	resp, err := client(c.VerifyTLS).Do(req)
	if err != nil {
		return 0, "", err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	return resp.StatusCode, string(body), nil
}

func (c *Creds) write(method, path string, payload any) (map[string]any, int, error) {
	buf, _ := json.Marshal(payload)
	req, _ := http.NewRequest(method, apiURL(c.BaseURL, path), bytes.NewReader(buf))
	req.Header.Set("Authorization", c.auth())
	req.Header.Set("Content-Type", "application/json")
	resp, err := client(c.VerifyTLS).Do(req)
	if err != nil {
		return nil, 0, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	var v map[string]any
	if err := json.Unmarshal(body, &v); err != nil {
		return nil, resp.StatusCode, fmt.Errorf("bad json: %s", limitStr(string(body), 200))
	}
	return v, resp.StatusCode, nil
}

func limitStr(s string, n int) string {
	if len(s) > n {
		return s[:n]
	}
	return s
}

// encodeSSHKey normalizes "type blob comment" -> url-encoded "type%20blob".
func encodeSSHKey(key string) string {
	fields := strings.Fields(key)
	if len(fields) < 2 {
		return url.QueryEscape(key)
	}
	joined := fields[0] + " " + fields[1]
	return strings.ReplaceAll(url.QueryEscape(joined), "+", "%20")
}

// TestConnection hits /version and classifies the result.
func (c *Creds) TestConnection() map[string]any {
	start := time.Now()
	v, code, err := c.get("/version", nil)
	ms := time.Since(start).Milliseconds()
	if err != nil {
		return map[string]any{"ok": false, "error": err.Error(), "latency_ms": ms, "hint": classifyErr(err)}
	}
	if code == 401 {
		return map[string]any{"ok": false, "error": "HTTP 401 Unauthorized", "latency_ms": ms,
			"hint": "Token salah / privilege separation on. Format user root@pam, token_id hanya setelah '!'."}
	}
	if code < 200 || code >= 300 {
		return map[string]any{"ok": false, "error": fmt.Sprintf("HTTP %d", code), "latency_ms": ms}
	}
	data, _ := v["data"].(map[string]any)
	ver, _ := data["version"].(string)
	rel, _ := data["release"].(string)
	return map[string]any{"ok": true, "version": ver, "release": rel, "latency_ms": ms}
}

func classifyErr(err error) string {
	s := strings.ToLower(err.Error())
	switch {
	case strings.Contains(s, "cert") || strings.Contains(s, "tls") || strings.Contains(s, "ssl"):
		return "TLS error (self-signed?). Matikan Verify TLS lalu coba lagi."
	case strings.Contains(s, "timeout") || strings.Contains(s, "deadline"):
		return "Timeout: IP/port salah, firewall, atau Proxmox tidak listen 8006."
	case strings.Contains(s, "connect") || strings.Contains(s, "refused") || strings.Contains(s, "no such host"):
		return "TCP connect gagal: IP/port salah atau firewall."
	default:
		return "Transport error — cek URL dan jaringan."
	}
}

// ListNodes returns live nodes.
func (c *Creds) ListNodes() ([]Node, error) {
	v, code, err := c.get("/nodes", nil)
	if err != nil {
		return nil, err
	}
	if code < 200 || code >= 300 {
		return nil, fmt.Errorf("HTTP %d", code)
	}
	var out []Node
	if arr, ok := v["data"].([]any); ok {
		for _, it := range arr {
			m, _ := it.(map[string]any)
			out = append(out, Node{
				Name:   strVal(m, "node"),
				Status: strVal(m, "status"),
				CPU:    floatVal(m, "cpu"),
				MaxMem: uintVal(m, "maxmem"),
				Mem:    uintVal(m, "mem"),
				Live:   true,
			})
		}
	}
	return out, nil
}

// ListVMsNode lists QEMU VMs on one node.
func (c *Creds) ListVMsNode(node string) ([]VM, error) {
	v, code, err := c.get("/nodes/"+node+"/qemu", nil)
	if err != nil {
		return nil, err
	}
	if code < 200 || code >= 300 {
		return nil, fmt.Errorf("HTTP %d", code)
	}
	return parseVMList(v, node), nil
}

// ListVMsCluster lists all VMs cluster-wide via /cluster/resources?type=vm.
func (c *Creds) ListVMsCluster() ([]VM, []string, error) {
	v, code, err := c.get("/cluster/resources", map[string]string{"type": "vm"})
	if err != nil {
		return nil, nil, err
	}
	if code < 200 || code >= 300 {
		return nil, nil, fmt.Errorf("HTTP %d", code)
	}
	var out []VM
	var nodes []string
	seen := map[string]bool{}
	if arr, ok := v["data"].([]any); ok {
		for _, it := range arr {
			m, _ := it.(map[string]any)
			typ, _ := m["type"].(string)
			if typ != "qemu" && typ != "lxc" {
				continue
			}
			node, _ := m["node"].(string)
			out = append(out, VM{
				VMID:   uintVal(m, "vmid"),
				Name:   strVal(m, "name"),
				Status: strVal(m, "status"),
				CPUs:   uint32(uintVal(m, "maxcpu")),
				CPU:    floatVal(m, "cpu"),
				Mem:    uintVal(m, "mem"),
				MaxMem: uintVal(m, "maxmem"),
				Uptime: uintVal(m, "uptime"),
				Node:   node,
			})
			// template flag
			out[len(out)-1].Template = templateFlag(m)
			if !seen[node] {
				seen[node] = true
				nodes = append(nodes, node)
			}
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	sort.Strings(nodes)
	return out, nodes, nil
}

// NextVMID returns next free VMID.
func (c *Creds) NextVMID() (uint64, error) {
	v, code, err := c.get("/cluster/nextid", nil)
	if err != nil {
		return 0, err
	}
	if code < 200 || code >= 300 {
		return 0, fmt.Errorf("HTTP %d", code)
	}
	switch d := v["data"].(type) {
	case string:
		var n uint64
		_, _ = fmt.Sscanf(d, "%d", &n)
		return n, nil
	case float64:
		return uint64(d), nil
	}
	return 0, fmt.Errorf("bad nextid response")
}

// CloneVM clones template -> new VM, returns UPID.
// Note: Proxmox rejects `storage` for linked clones, so it is only sent on full clones.
func (c *Creds) CloneVM(node string, templateVMID, newID uint64, name string, full bool, storage string) (string, error) {
	body := map[string]any{"newid": newID, "name": name, "target": node}
	if full {
		body["full"] = 1
		if storage != "" {
			body["storage"] = storage
		}
	} else {
		body["full"] = 0
	}
	v, code, err := c.post(fmt.Sprintf("/nodes/%s/qemu/%d/clone", node, templateVMID), body)
	if err != nil {
		return "", err
	}
	if code < 200 || code >= 300 {
		msg := ""
		if em, ok := v["message"].(string); ok {
			msg = strings.TrimSpace(em)
		} else if ee, ok := v["errors"]; ok {
			msg = fmt.Sprintf("%v", ee)
		}
		if msg == "" {
			msg = fmt.Sprintf("HTTP %d", code)
		}
		return "", fmt.Errorf("clone: %s", msg)
	}
	upid, _ := v["data"].(string)
	return upid, nil
}

// WaitTask polls until task stops (max ~5 min). Returns exit status.
func (c *Creds) WaitTask(node, upid string) (string, error) {
	enc := url.PathEscape(upid)
	for i := 0; i < 100; i++ {
		v, code, err := c.get(fmt.Sprintf("/nodes/%s/tasks/%s/status", node, enc), nil)
		if err != nil {
			return "", err
		}
		if code < 200 || code >= 300 {
			return "", fmt.Errorf("HTTP %d", code)
		}
		data, _ := v["data"].(map[string]any)
		if status, _ := data["status"].(string); status != "stopped" {
			time.Sleep(3 * time.Second)
			continue
		}
		exit, _ := data["exitstatus"].(string)
		return exit, nil
	}
	return "", fmt.Errorf("timeout menunggu task %s", upid)
}

// SetVMConfig writes cloud-init (ciuser/sshkeys/ipconfig/nameserver).
func (c *Creds) SetVMConfig(node string, vmid uint64, ciuser, sshkeys, ipconfig, nameserver string) error {
	body := map[string]any{"ciuser": ciuser, "ipconfig0": ipconfig, "nameserver": nameserver}
	if sshkeys != "" {
		// PVE 9: sshkeys harus URL-encoded (%20) dan TANPA comment (@ merusak decoder).
		body["sshkeys"] = encodeSSHKey(sshkeys)
	}
	_, code, err := c.put(fmt.Sprintf("/nodes/%s/qemu/%d/config", node, vmid), body)
	if err != nil {
		return err
	}
	if code < 200 || code >= 300 {
		return fmt.Errorf("HTTP %d", code)
	}
	return nil
}

// Power sends start|shutdown|reboot|stop.
func (c *Creds) Power(node string, vmid uint64, action string) (string, error) {
	v, code, err := c.post(fmt.Sprintf("/nodes/%s/qemu/%d/status/%s", node, vmid, action), map[string]any{})
	if err != nil {
		return "", err
	}
	if code < 200 || code >= 300 {
		return "", fmt.Errorf("HTTP %d", code)
	}
	upid, _ := v["data"].(string)
	return upid, nil
}

// AgentIP returns first routable IPv4 via guest agent, or "".
func (c *Creds) AgentIP(node string, vmid uint64) string {
	v, code, err := c.post(fmt.Sprintf("/nodes/%s/qemu/%d/agent/network-get-interfaces", node, vmid), map[string]any{})
	if err != nil || code < 200 || code >= 300 {
		return ""
	}
	data, _ := v["data"].(map[string]any)
	arr, _ := data["result"].([]any)
	for _, it := range arr {
		iface, _ := it.(map[string]any)
		if name, _ := iface["name"].(string); name == "lo" {
			continue
		}
		addrs, _ := iface["ip-addresses"].([]any)
		for _, a := range addrs {
			am, _ := a.(map[string]any)
			ip, _ := am["ip-address"].(string)
			if ip == "" || strings.Contains(ip, ":") || strings.HasPrefix(ip, "127.") || strings.HasPrefix(ip, "169.254.") {
				continue
			}
			return ip
		}
	}
	return ""
}

// DeleteVM removes a stopped VM.
func (c *Creds) DeleteVM(node string, vmid uint64) error {
	code, body, err := c.del(fmt.Sprintf("/nodes/%s/qemu/%d", node, vmid), "?destroy-unreferenced-disks=1&purge=1")
	if err != nil {
		return err
	}
	if code < 200 || code >= 300 {
		return fmt.Errorf("HTTP %d: %s", code, limitStr(body, 200))
	}
	return nil
}

func templateFlag(m map[string]any) bool {
	switch t := m["template"].(type) {
	case bool:
		return t
	case float64:
		return t == 1
	case string:
		return t == "1" || strings.EqualFold(t, "true")
	}
	return false
}

func parseVMList(v map[string]any, node string) []VM {
	var out []VM
	if arr, ok := v["data"].([]any); ok {
		for _, it := range arr {
			m, _ := it.(map[string]any)
			out = append(out, VM{
				VMID:     uintVal(m, "vmid"),
				Name:     strVal(m, "name"),
				Status:   strVal(m, "status"),
				CPUs:     uint32(uintVal(m, "cpus")),
				CPU:      floatVal(m, "cpu"),
				Mem:      uintVal(m, "mem"),
				MaxMem:   uintVal(m, "maxmem"),
				Uptime:   uintVal(m, "uptime"),
				Template: templateFlag(m),
				Node:     node,
			})
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

func strVal(m map[string]any, k string) string {
	s, _ := m[k].(string)
	if s == "" {
		return "?"
	}
	return s
}

func floatVal(m map[string]any, k string) float64 {
	f, _ := m[k].(float64)
	return f
}

func uintVal(m map[string]any, k string) uint64 {
	switch n := m[k].(type) {
	case float64:
		return uint64(n)
	case string:
		var v uint64
		_, _ = fmt.Sscanf(n, "%d", &v)
		return v
	}
	return 0
}
