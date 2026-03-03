CXX = c++
CXXFLAGS = -std=c++17 $(shell pkg-config --cflags protobuf)
LDFLAGS = $(shell pkg-config --libs protobuf)

test: main.cc hello.pb.cc hello.pb.h
	$(CXX) $(CXXFLAGS) -o test main.cc hello.pb.cc $(LDFLAGS)

clean:
	rm -f test

.PHONY: clean
