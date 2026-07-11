# Wallpaper Engine tooling — install/uninstall/build (fork layout)
# In this fork the engine sources live at the repo root; the tooling layer is
# under tooling/ (daemon + gallery) and desktop/ (Tauri 2 shell).

SHELL   := /bin/bash
HOME    ?= $(shell echo $$HOME)
BIN     := $(HOME)/.local/bin
UNITDIR := $(HOME)/.config/systemd/user
GALLERY := $(HOME)/wallpapers.html
ENGINE_PREFIX := /opt/linux-wallpaperengine

SCRIPTS := lwe-index.py lwe-daemon.py lwe-launch lwe-select

.PHONY: help install uninstall build-engine patch-engine status

help:
	@echo "targets:"
	@echo "  make install       - install scripts, gallery, systemd unit; enable daemon"
	@echo "  make uninstall     - remove installed scripts, gallery, unit; disable daemon"
	@echo "  make build-engine  - cmake+make the engine at repo root, then sudo make install to $(ENGINE_PREFIX)"
	@echo "  make patch-engine  - re-apply patches/cef-fixes.patch to a clean checkout"
	@echo "  make status        - show what's installed"

install:
	@mkdir -p "$(BIN)" "$(UNITDIR)"
	@for s in $(SCRIPTS); do \
		install -m 0755 "tooling/$$s" "$(BIN)/$$s" && echo "installed $(BIN)/$$s"; \
	done
	@install -m 0644 tooling/wallpapers.html "$(GALLERY)" && echo "installed $(GALLERY)"
	@install -m 0644 tooling/lwe-daemon.service "$(UNITDIR)/lwe-daemon.service" && echo "installed unit"
	@systemctl --user daemon-reload
	@systemctl --user enable --now lwe-daemon.service && echo "daemon enabled+started"
	@echo "done. open: firefox file://$(GALLERY)"

uninstall:
	@for s in $(SCRIPTS); do rm -f "$(BIN)/$$s" && echo "removed $(BIN)/$$s"; done
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
	@echo "scripts:"; for s in $(SCRIPTS); do \
		[ -e "$(BIN)/$$s" ] && echo "  [x] $$s" || echo "  [ ] $$s"; done
	@echo "gallery:  $$([ -e "$(GALLERY)" ] && echo present || echo missing)"
	@echo "unit:     $$([ -e "$(UNITDIR)/lwe-daemon.service" ] && echo present || echo missing)"
	@echo "daemon:   $$(systemctl --user is-active lwe-daemon.service 2>/dev/null)"
	@echo "engine:   $$([ -x "$(ENGINE_PREFIX)/linux-wallpaperengine" ] && echo installed || echo missing)"
