# Development helpers. Releases are built by .github/workflows/build.yml.
WAILS3 ?= wails3
# Linux development builds use GTK 3 / WebKitGTK 4.1.
LINUX_TAGS = gtk3

.PHONY: frontend bindings mac linux test ui-test

frontend:
	cd frontend && npm run build

# Regenerate frontend/bindings after changing App's exported methods.
# Install the tool with: go install github.com/wailsapp/wails/v3/cmd/wails3@v3.1.0
bindings:
	$(WAILS3) generate bindings -clean -ts -i $(if $(filter Linux,$(shell uname)),-f '-tags $(LINUX_TAGS)') -d frontend/bindings

mac: frontend
	scripts/build-mac.sh

linux: frontend
	go build -tags $(LINUX_TAGS) -o build/bin/jamalterm .

test:
	go test ./internal/...

ui-test: frontend
	cd frontend && npm run test:ui
