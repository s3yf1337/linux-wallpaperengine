# Wallpaper Engine tooling — install/uninstall/build (fork layout)
# Engine sources at repo root; tooling/ (shell helpers + gallery) and desktop/ (Tauri 2 + Rust daemon).

SHELL   := /bin/bash
HOME    ?= $(shell echo $$HOME)
BIN     := $(HOME)/.local/bin
UNITDIR := $(HOME)/.config/systemd/user
GALLERY := $(HOME)/wallpapers.html
ENGINE_PREFIX := /opt/linux-wallpaperengine
DESKTOP := desktop/src-tauri
CARGO   := cargo

# Rust binaries (replace the old Python daemon/index)
RUST_BINS := lwe-daemon lwe-index
# Thin shell helpers
SHELL_SCRIPTS := lwe-launch lwe-select

.PHONY: help install uninstall build-engine build-tooling patch-engine status

help:
	@echo "targets:"
	@echo "  make install        - build Rust daemon/index, install scripts+unit+gallery, enable daemon"
	@echo "  make uninstall      - remove installed scripts/unit; disable daemon"
	@echo "  make build-tooling  - cargo build --release lwe-daemon + lwe-index"
	@echo "  make build-engine   - cmake+make engine at repo root, then sudo make install to $(ENGINE_PREFIX)"
	@echo "  make patch-engine   - re-apply patches/cef-fixes.patch to a clean checkout"
	@echo "  make status         - show what's installed"

build-tooling:
	@cd $(DESKTOP) && $(CARGO) build --release --bin lwe-daemon --bin lwe-index
	@echo "built: $(DESKTOP)/target/release/lwe-daemon lwe-index"

install: build-tooling
	@mkdir -p "$(BIN)" "$(UNITDIR)"
	@for s in $(RUST_BINS); do \
		install -m 0755 "$(DESKTOP)/target/release/$$s" "$(BIN)/$$s" && echo "installed $(BIN)/$$s"; \
	done
	@for s in $(SHELL_SCRIPTS); do \
		install -m 0755 "tooling/$$s" "$(BIN)/$$s" && echo "installed $(BIN)/$$s"; \
	done
	@install -m 0644 tooling/wallpapers.html "$(GALLERY)" && echo "installed $(GALLERY)"
	@install -m 0644 tooling/lwe-daemon.service "$(UNITDIR)/lwe-daemon.service" && echo "installed unit"
	@systemctl --user daemon-reload
	@systemctl --user enable --now lwe-daemon.service && echo "daemon enabled+started"
	@echo "done. open: firefox file://$(GALLERY)  or  cd desktop && npm run tauri dev"

uninstall:
	@for s in $(RUST_BINS) $(SHELL_SCRIPTS); do rm -f "$(BIN)/$$s" && echo "removed $(BIN)/$$s"; done
	@# drop legacy python installs if any
	@rm -f "$(BIN)/lwe-daemon.py" "$(BIN)/lwe-index.py"
	@systemctl --user disable --now lwe-daemon.service 2>/dev/null || true
	@rm -f "$(UNITDIR)/lwe-daemon.service"
	@systemctl --user daemon-reload
	@echo "kept (remove by hand if you want): $(GALLERY), ~/.cache/lwe_index.json"

build-engine:
	@mkdir -p build && cd build && \
		cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$(ENGINE_PREFIX) && \
		make -j$$(nproc) && \
		sudo make install
	@echo "engine installed to $(ENGINE_PREFIX)"

patch-engine:
	@git apply --check patches/cef-fixes.patch && \
		git apply patches/cef-fixes.patch && echo "patch applied" || \
		echo "patch failed / already applied"

status:
	@echo "scripts:"; for s in $(RUST_BINS) $(SHELL_SCRIPTS); do \
		[ -e "$(BIN)/$$s" ] && echo "  [x] $$s" || echo "  [ ] $$s"; done
	@echo "legacy python:"; for s in lwe-daemon.py lwe-index.py; do \
		[ -e "$(BIN)/$$s" ] && echo "  [!] $$s STILL INSTALLED" || echo "  [ ] $$s (gone)"; done
	@echo "gallery:  $$([ -e "$(GALLERY)" ] && echo present || echo missing)"
	@echo "unit:     $$([ -e "$(UNITDIR)/lwe-daemon.service" ] && echo present || echo missing)"
	@echo "daemon:   $$(systemctl --user is-active lwe-daemon.service 2>/dev/null)"
	@echo "engine:   $$([ -x "$(ENGINE_PREFIX)/linux-wallpaperengine" ] && echo installed || echo missing)"
