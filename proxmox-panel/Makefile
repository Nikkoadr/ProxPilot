.PHONY: all install build run clean

GO_DIR  := backend
FE_DIR  := frontend
PORT    ?= 8080

all: install build run

install:
	@echo "[1/4] Installing frontend deps..."
	cd $(FE_DIR) && npm install
	@echo "[2/4] Installing Go deps..."
	cd $(GO_DIR) && go mod tidy

build:
	@echo "[3/4] Building frontend..."
	cd $(FE_DIR) && npm run build

run:
	@echo "[4/4] Starting backend on :$(PORT)..."
	cd $(GO_DIR) && go run main.go

dev:
	@echo "Starting frontend dev server on :5173..."
	cd $(FE_DIR) && npm run dev &
	cd $(GO_DIR) && go run main.go

clean:
	rm -rf $(FE_DIR)/dist $(FE_DIR)/node_modules $(GO_DIR)/go.sum $(GO_DIR)/vendor
