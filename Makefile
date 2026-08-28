.PHONY: demo check rust-check python-check

demo:
	@bash scripts/demo.sh

check: rust-check python-check

rust-check:
	@cd clared-core && cargo fmt --all -- --check
	@cd clared-core && cargo clippy --all-targets -- -D warnings
	@cd clared-core && cargo test

python-check:
	@if command -v uv >/dev/null 2>&1; then \
		cd clared-python && uv run --extra dev pytest -v; \
	else \
		python3 -m venv clared-python/.venv && \
		clared-python/.venv/bin/python -m pip install --quiet -e "./clared-python[dev]" && \
		clared-python/.venv/bin/python -m pytest -v clared-python/tests; \
	fi
