#!/bin/bash

# 1. Start by kicking off a bunch of services
./target/release/examples/distr-tps &
SERVICE_0_PID=$!

./target/release/examples/distr-tps -p 7622 -n "127.0.0.1:7621" &
SERVICE_1_PID=$!

./target/release/examples/distr-tps -p 7623 -n "127.0.0.1:7621" &
SERVICE_2_PID=$!

./target/release/examples/distr-tps -p 7624 -n "127.0.0.1:7621" &
SERVICE_3_PID=$!

./target/release/examples/distr-tps -p 7625 -n "127.0.0.1:7621" &
SERVICE_4_PID=$!


# 2. Define a cleanup function & trap so we can exit early if needed.
function cleanup() {
    echo "Shutting down..."
    kill $SERVICE_0_PID
    kill $SERVICE_1_PID
    kill $SERVICE_2_PID
    kill $SERVICE_3_PID
    kill $SERVICE_4_PID
    echo "Cleanup complete."
}

trap 'cleanup' SIGINT

tail --pid=$SERVICE_0_PID -f /dev/null

# 5. Manually call cleanup.
cleanup
