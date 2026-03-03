#include <cassert>
#include <cmath>
#include <cstdio>
#include <string>

#include "hello.pb.h"

int main() {
    // Create a Hello message.
    termsurf::Hello original;
    original.set_name("TermSurf");
    original.set_id(-42);
    original.set_size(1024);
    original.set_x(3.14);
    original.set_active(true);

    // Serialize.
    std::string bytes;
    original.SerializeToString(&bytes);

    // Deserialize.
    termsurf::Hello decoded;
    assert(decoded.ParseFromString(bytes));

    // Verify fields.
    assert(decoded.name() == "TermSurf");
    assert(decoded.id() == -42);
    assert(decoded.size() == 1024);
    assert(std::fabs(decoded.x() - 3.14) < 1e-15);
    assert(decoded.active() == true);

    printf("C++: pass (%zu bytes)\n", bytes.size());
    return 0;
}
