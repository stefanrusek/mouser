// OWNER-RUN probe (not run by the planner: it changes live cursor state and needs a human watching).
// Emulates what SDL/GLFW do when a game captures the mouse, so the owner can see how Mouser and a
// capturing app interact WITHOUT a game installed. Run it from Terminal.
//
//   cc spike_foreign_capture.c -framework ApplicationServices -o spike_foreign_capture
//   ./spike_foreign_capture capture-while-remote   # S1/S2/S3: run while the pointer is on the other computer
//   ./spike_foreign_capture release-while-remote   # same, then releases after 5 s
//   ./spike_foreign_capture hold                   # S4: capture, then move the pointer onto the Mac and watch
//
// Every mode prints one line per 250 ms: visible, position, and the hide calls it made. It always
// ends by undoing its own calls (associate on, show cursor as often as it hid) and also on Ctrl-C.
// Output goes to spike_foreign_capture.<mode>.out beside this file; the owner pastes the verdict
// (what was seen on screen) into spikes/FINDINGS.md.
#include <ApplicationServices/ApplicationServices.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
extern boolean_t CGCursorIsVisible(void);
static int hides = 0;
static volatile sig_atomic_t stop = 0;
static void on_int(int s) { (void)s; stop = 1; }
static CGPoint where_(void) {
    CGEventRef e = CGEventCreate(NULL); CGPoint p = CGEventGetLocation(e); CFRelease(e); return p;
}
static void capture(void) {           // what a windowing library does on "disable cursor"
    CGPoint c = CGPointMake(CGDisplayPixelsWide(CGMainDisplayID()) / 2.0, CGDisplayPixelsHigh(CGMainDisplayID()) / 2.0);
    CGAssociateMouseAndMouseCursorPosition(0);
    CGDisplayHideCursor(kCGDirectMainDisplay); hides++;
    CGWarpMouseCursorPosition(c);
}
static void release(void) {
    CGAssociateMouseAndMouseCursorPosition(1);
    while (hides > 0) { CGDisplayShowCursor(kCGDirectMainDisplay); hides--; }
}
int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "capture-while-remote";
    signal(SIGINT, on_int);
    char path[128]; snprintf(path, sizeof path, "spike_foreign_capture.%s.out", mode);
    FILE *out = fopen(path, "w");
    for (int t = 0; t < 80 && !stop; t++) {            // 20 seconds
        if (t == 4) { capture(); fprintf(out, "t=%dms CAPTURE\n", t * 250); }
        if (!strcmp(mode, "release-while-remote") && t == 24) { release(); fprintf(out, "t=%dms RELEASE\n", t * 250); }
        if (!strcmp(mode, "capture-while-remote") && t % 8 == 7) { capture(); fprintf(out, "t=%dms RECAPTURE\n", t * 250); }
        CGPoint p = where_();
        fprintf(out, "t=%dms visible=%d pos=(%.0f,%.0f) our_hides=%d\n", t * 250, CGCursorIsVisible(), p.x, p.y, hides);
        fflush(out); usleep(250000);
    }
    release(); fprintf(out, "RELEASED all\n"); fclose(out);
    return 0;
}
