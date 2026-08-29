#include <cassert>
#include <cstdio>
#include <string>

#include "termsurf.pb.h"

int main() {
    // Create a TermSurfMessage wrapping a CreateTab.
    termsurf::TermSurfMessage original;
    auto* tab = original.mutable_create_tab();
    tab->set_url("https://termsurf.com");
    tab->set_pane_id("pane-1");
    tab->set_pixel_width(1920);
    tab->set_pixel_height(1080);
    tab->set_dark(true);

    // Serialize.
    std::string bytes;
    original.SerializeToString(&bytes);

    // Deserialize.
    termsurf::TermSurfMessage decoded;
    assert(decoded.ParseFromString(bytes));

    // Verify the oneof round-trips correctly.
    assert(decoded.msg_case() == termsurf::TermSurfMessage::kCreateTab);
    const auto& ct = decoded.create_tab();
    assert(ct.url() == "https://termsurf.com");
    assert(ct.pane_id() == "pane-1");
    assert(ct.pixel_width() == 1920);
    assert(ct.pixel_height() == 1080);
    assert(ct.dark() == true);

    printf("C++: pass (%zu bytes)\n", bytes.size());
    return 0;
}
