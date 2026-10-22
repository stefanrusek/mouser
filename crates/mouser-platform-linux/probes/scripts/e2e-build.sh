#!/bin/bash
# Build the e2e binaries from a VM-local copy of the repo. The copy carries one
# workaround: GTK has no window to apply "ignore cursor events" to until it is
# shown, so the overlay applies it after show (the unpatched app panics at startup).
set -eu
mkdir -p ~/src
rsync -a --delete --exclude target --exclude .git --exclude node_modules /Users/stefanrusek/Code/mouser/ ~/src/
cd ~/src
python3 - <<'P'
p='apps/mouser-app/src/overlay.rs'
s=open(p).read()
s=s.replace('''    let window = WebviewWindowBuilder::new(app, LABEL''','''    WebviewWindowBuilder::new(app, LABEL''')
s=s.replace('''    window
        .set_ignore_cursor_events(true)
        .expect("landing overlay ignores the pointer");
}''','}')
s=s.replace('''    window.show().expect("landing overlay can be shown");''','''    window.show().expect("landing overlay can be shown");
    window.set_ignore_cursor_events(true).expect("ignore");''')
open(p,'w').write(s)
P
export CARGO_TARGET_DIR=$HOME/target
cargo build -p mouser-app --features gui -p mouser-cli 2>&1 | tail -3
