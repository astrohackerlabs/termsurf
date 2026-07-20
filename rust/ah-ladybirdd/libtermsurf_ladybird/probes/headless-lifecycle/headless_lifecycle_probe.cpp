#include <LibCore/AnonymousBuffer.h>
#include <LibCore/EventLoop.h>
#include <LibWeb/PixelUnits.h>
#include <LibWebView/HeadlessWebView.h>

int main(int argc, char** argv)
{
    Core::EventLoop event_loop;

    auto theme_buffer = Core::AnonymousBuffer::create_with_size(4096).release_value();
    auto view = WebView::HeadlessWebView::create(
        move(theme_buffer),
        Web::DevicePixelSize { 800, 600 });

    view->on_load_start = [](auto const&, bool) {};
    view->on_load_finish = [](auto const&) {};
    view->on_title_change = [](auto const&) {};

    if (argc > 1)
        view->load(URL::URL::create_with_url_or_path(argv[1]).release_value());

    return 0;
}
