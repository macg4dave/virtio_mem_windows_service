BINARY := virtio-mem-controller

.PHONY: all build test vet fmt clean

all: build

build:
	go build -o $(BINARY) ./cmd/controller

test:
	go test ./...

vet:
	go vet ./...

fmt:
	gofmt -w .

clean:
	rm -f $(BINARY)
