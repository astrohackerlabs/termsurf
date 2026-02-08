CXX = clang++
CXXFLAGS = -std=c++17 -Wall -Wextra -fobjc-arc
FRAMEWORKS = -framework Metal -framework IOSurface -framework CoreFoundation -framework Foundation
TARGET = termsurf-browser

SRCS = src/main.mm
OBJS = $(SRCS:.mm=.o)

$(TARGET): $(OBJS)
	$(CXX) $(CXXFLAGS) $(FRAMEWORKS) -o $@ $^

src/%.o: src/%.mm
	$(CXX) $(CXXFLAGS) -c -o $@ $<

clean:
	rm -f $(TARGET) $(OBJS)

.PHONY: clean
