// C imports here are exposed to Swift.

#import "ObjCExceptionCatcher.h"
#import "VibrantLayer.h"

void termsurf_pane_closed(const char *pane_id);
void termsurf_pane_focus_changed(const char *pane_id, int focused);
void termsurf_gui_active_changed(int active);
void termsurf_hello_config_changed(const char *homepage, const char *browsers);
void termsurf_color_scheme_changed(int dark);
