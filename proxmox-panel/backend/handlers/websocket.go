package handlers

import (
	"encoding/json"
	"net/http"
	"sync"

	"github.com/gorilla/websocket"
	"proxmox-panel/services"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

type wsClient struct {
	conn *websocket.Conn
}

var (
	clients   = make(map[string]map[*wsClient]bool)
	clientsMu sync.RWMutex
)

func NewWebSocketHandler(store *services.ClusterStore) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id := r.PathValue("id")
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		client := &wsClient{conn: conn}
		clientsMu.Lock()
		if clients[id] == nil {
			clients[id] = make(map[*wsClient]bool)
		}
		clients[id][client] = true
		clientsMu.Unlock()

		sendInitialLogs(store, id, client)

		defer func() {
			clientsMu.Lock()
			delete(clients[id], client)
			clientsMu.Unlock()
			conn.Close()
		}()

		for {
			_, msg, err := conn.ReadMessage()
			if err != nil {
				break
			}
			var req struct {
				Action string `json:"action"`
			}
			json.Unmarshal(msg, &req)
			if req.Action == "subscribe" {
				sendInitialLogs(store, id, client)
			}
		}
	}
}

func sendInitialLogs(store *services.ClusterStore, clusterID string, client *wsClient) {
	state, ok := store.GetCluster(clusterID)
	if !ok {
		return
	}
	msg, _ := json.Marshal(map[string]interface{}{
		"type":       "logs",
		"cluster_id": clusterID,
		"logs":       state.Logs,
		"status":     state.Status,
		"progress":   state.Progress,
	})
	client.conn.WriteMessage(websocket.TextMessage, msg)
}
