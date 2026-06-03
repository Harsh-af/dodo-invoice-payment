.PHONY: build up down logs test demo health

build:
	docker compose build

rebuild:
	docker compose build --no-cache

up:
	docker compose up -d

down:
	docker compose down

upd: build up

restart: down build up

logs:
	docker compose logs -f

health:
	curl -s http://localhost:8080/health

test:
	set INTEGRATION_TEST=1&& cargo test -p invoice-service --test integration

demo: up
	@echo Waiting for API...
	@powershell -Command "Start-Sleep 5; $$h = Invoke-RestMethod http://localhost:8080/health; Write-Host health: $$h"