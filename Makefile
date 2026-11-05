.PHONY: all test check preview clean
all:
	bash scripts/build.sh
test:
	cargo +stable test --offline --lib
	bash scripts/test_audio.sh
	python3 scripts/verify_media.py
check:
	cargo +stable fmt --check
	cargo +stable clippy --offline --all-targets -- -D warnings
	bash scripts/build.sh clippy
preview:
	cargo +stable run --offline --bin preview
clean:
	cargo +stable clean
