{
    "hostio": function(info) {
        if (this.nest) {
            info.info = this.open.pop();
            this.nest = false;
        }
        this.open.push(info);
    },
    "enter": function(frame) {
        let inner = [];
        this.open.push({
            address: frame.getTo(),
            steps: inner,
        });

        this.stack.push(this.open); // save where we were
        this.open = inner;
    },
    "exit": function() {
        this.open = this.stack.pop();
        this.nest = true;
    },
    "result": function() { return this.open; },
    "fault":  function() { return this.open; },
    stack: [],
    open: [],
    nest: false
}
