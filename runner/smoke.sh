#!/bin/zsh
# End-to-end smoke test of the Rust exit node through the local Go test client
# (state/go-client, SOCKS5 on 127.0.0.1:1055). Prints one line per check, PASS/FAIL, exit 1 on failure.
# usage: runner/smoke.sh [expected_public_ip]
set -u
X=(--socks5-hostname 127.0.0.1:1055)
want_ip=${1:-$(curl -s -4 -m 10 https://ifconfig.me)}
fail=0
check() { if eval "$2"; then echo "PASS $1"; else echo "FAIL $1"; fail=1; fi; }
${0:A:h}/tsc set --exit-node=rust-exit
ip=$(curl -s -m 15 $X https://ifconfig.me)
check "egress ip $ip == $want_ip" '[[ $ip == $want_ip ]]'
code=$(curl -s -m 15 $X -o /dev/null -w '%{http_code}' https://example.com)
check "dns+https example.com ($code)" '[[ $code == 200 ]]'
code=$(curl -s -m 15 $X -o /dev/null -w '%{http_code}' http://example.com/)
check "plain http example.com ($code)" '[[ $code == 200 ]]'
# Apple's networkQuality CDN: built for load tests (Cloudflare and OVH rate-limit repeated runs from one IP).
bps=$(curl -s -m 60 $X -o /dev/null -w '%{speed_download}' -r 0-52428799 https://mensura.cdn-apple.com/api/v1/gm/large)
check "50 MB download ($(( ${bps%.*} * 8 / 1000000 )) Mbit/s)" '[[ ${bps%.*} -gt 1000000 ]]'
t=$(curl -s -m 8 $X -o /dev/null -w '%{time_total}' 'https://[2606:4700:4700::1111]/'); rc=$?
check "ipv6 refused fast (rc=$rc, ${t}s)" '[[ $rc -ne 0 ]] && (( ${t%.*} < 3 ))'
body=$(curl -s -m 8 $X http://192.168.50.1/ | head -c 64)
check "home LAN unreachable" '[[ -z $body ]]'
exit $fail
