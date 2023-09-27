
json=$(cat src/query.js | jq -sR .)

curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"debug_traceTransaction","params":["0x96169d2a8e0357b4aa61222d85eec1139953f551a04d0188576cf65ce358f495", { "tracer": '"$json"' }],"id":1}' http://localhost:8545 | jq
