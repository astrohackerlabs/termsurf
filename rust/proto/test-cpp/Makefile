CXX = c++
CXXFLAGS = -std=c++17 $(shell pkg-config --cflags protobuf)
LDFLAGS = $(shell pkg-config --libs protobuf)

test: main.cc termsurf.pb.cc termsurf.pb.h
	$(CXX) $(CXXFLAGS) -o test main.cc termsurf.pb.cc $(LDFLAGS)

clean:
	rm -f test

.PHONY: clean
