all: comp1

j.%: prj3
	docker compose -f jeremy_tests/$*.yml up

comp%: prj3
	docker compose -f testcases/docker-compose-testcase-$*.yml up

prj3:
	docker build . -t $@

.PHONY: teardown%
teardown%:
	docker compose -f testcases/docker-compose-testcase-$*.yml down
