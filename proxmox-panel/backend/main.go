package main

import (
	"log"
	"net/http"
	"os"
	"path/filepath"

	"github.com/gorilla/mux"
	"proxmox-panel/handlers"
	"proxmox-panel/services"
)

func main() {
	baseDir, _ := os.Getwd()
	infraDir := filepath.Join(baseDir, "..", "infra")
	terraformDir := filepath.Join(infraDir, "terraform")

	os.MkdirAll(terraformDir, 0755)

	store := services.NewClusterStore()
	manager := services.NewClusterManager(store, terraformDir, "")
	handler := handlers.NewHandler(manager, store)

	r := mux.NewRouter()

	api := r.PathPrefix("/api").Subrouter()
	api.HandleFunc("/clusters", handler.GetClusters).Methods("GET")
	api.HandleFunc("/clusters", handler.CreateCluster).Methods("POST")
	api.HandleFunc("/clusters/{id}/deploy", handler.DeployCluster).Methods("POST")
	api.HandleFunc("/clusters/{id}/status", handler.GetClusterStatus).Methods("GET")
	api.HandleFunc("/clusters/{id}", handler.DeleteCluster).Methods("DELETE")
	api.HandleFunc("/nodes", handler.GetNodes).Methods("GET")
	api.HandleFunc("/templates", handler.GetTemplates).Methods("GET")
	api.HandleFunc("/config", handler.GetConfig).Methods("GET")
	api.HandleFunc("/ws/logs/{id}", handlers.NewWebSocketHandler(store)).Methods("GET")

	frontendDir := filepath.Join(baseDir, "..", "frontend", "dist")
	fs := http.FileServer(http.Dir(frontendDir))
	r.PathPrefix("/").Handler(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		path := filepath.Join(frontendDir, r.URL.Path)
		if _, err := os.Stat(path); err == nil {
			fs.ServeHTTP(w, r)
		} else {
			http.ServeFile(w, r, filepath.Join(frontendDir, "index.html"))
		}
	}))

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	log.Printf("Starting Proxmox Panel on :%s", port)
	log.Printf("Open http://localhost:%s in your browser", port)
	log.Fatal(http.ListenAndServe(":"+port, r))
}
