// Read-only probe: is the Mac cursor visible, and does it move? Changes no cursor state.
// cc spike_cursor_state.c -framework CoreGraphics -framework CoreFoundation -o spike_cursor_state
#include <ApplicationServices/ApplicationServices.h>
#include <stdio.h>
#include <unistd.h>
extern boolean_t CGCursorIsVisible(void);
int main(void) {
    for (int i = 0; i < 6; i++) {
        CGEventRef e = CGEventCreate(NULL);
        CGPoint p = CGEventGetLocation(e);
        CFRelease(e);
        printf("visible=%d pos=(%.1f,%.1f)\n", CGCursorIsVisible(), p.x, p.y);
        usleep(500000);
    }
    return 0;
}
