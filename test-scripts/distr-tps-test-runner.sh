#!/bin/bash

# 1. Start by kicking off both services, the test-service which will act
#    as a pretend host, and distr which is the load-test suite.
./target/release/mock-service &
MOCK_SERVICE_PID=$!

./target/release/examples/distr-tps -n "127.0.0.1:7622" &
SERVICE_0_PID=$!

./target/release/examples/distr-tps -p 7622 -n "127.0.0.1:7621" &
SERVICE_1_PID=$!


# 2. Define a cleanup function & trap so we can exit early if needed.
function cleanup() {
    echo "Shutting down..."
    kill $MOCK_SERVICE_PID
    kill $SERVICE_0_PID
    kill $SERVICE_1_PID
    echo "Cleanup complete."
}

trap 'cleanup' SIGINT


# 3. Apply a cpulimit on the distr service to simulate lack of resources
#    and trigger distributed behavior.
cpulimit -l 15 -p $SERVICE_0_PID &
cpulimit -l 15 -p $SERVICE_1_PID &


# 4. Kick off the load tests suite with a curl command, and then wait on it.
sleep 1
curl -XPOST "0.0.0.0:7621/run" --json '{ "name": "scenario_a", "duration": 120, "kind": { "Tps": 500  }}'

tail --pid=$MOCK_SERVICE_PID -f /dev/null


# 5. Manually call cleanup.
cleanup
