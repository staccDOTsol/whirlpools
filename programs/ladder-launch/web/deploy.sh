#!/bin/sh
# Stamp script URLs with a version so browsers never run a stale app.js after a deploy,
# then deploy to production. Run from this directory.
set -e
V=$(date +%s)
sed -i '' -E "s#(\./app\.js)(\?v=[0-9]+)?#\1?v=$V#" index.html
sed -i '' -E "s#(\./chain\.js)(\?v=[0-9]+)?#\1?v=$V#" app.js
vercel deploy --prod --yes "$@"
