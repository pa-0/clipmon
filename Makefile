DESTDIR?=/
PREFIX=/usr

clipmon:
	cargo build --release

build: target/release/clipmon

install: build
	@install -Dm755 target/release/clipmon ${DESTDIR}${PREFIX}/lib/clipmon
	@install -Dm644 clipmon.service ${DESTDIR}${PREFIX}/lib/systemd/user/clipmon.service

.PHONY: build install
