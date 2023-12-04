#!/bin/bash

# 1. Start by kicking off both services, the test-service which will act
#    as a pretend host, and basic which is the load-test suite.
./target/release/mock-service &
MOCK_SERVICE_PID=$!

./target/release/examples/${1} &
SERVICE_0_PID=$!


# 2. Define a cleanup function & trap so we can exit early if needed.
function cleanup() {
    echo "Shutting down..."
    kill $MOCK_SERVICE_PID
    kill $SERVICE_0_PID
    echo "Cleanup complete."
}

trap 'cleanup' SIGINT

#cpulimit -l 15 -p $SERVICE_0_PID &

# 3. Wait on mock service
tail --pid=$SERVICE_0_PID

# 4. Manually call cleanup.
cleanup
