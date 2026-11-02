// Appended to ghostty src/terminal/osc/parsers/program_status.zig by run.sh.
// Prints one TSDIFF line per body in termstate_diff_input.txt (hex, one per
// line, prefixed with x) describing what Ghostty parsed.


fn tsHex(label: []const u8, v: ?[]const u8) void {
    if (v) |s| {
        std.debug.print(" {s}=", .{label});
        for (s) |c| std.debug.print("{x:0>2}", .{c});
        if (s.len == 0) std.debug.print("~", .{});
    } else std.debug.print(" {s}=-", .{label});
}

test "termstate differential" {
    const input = @embedFile("termstate_diff_input.txt");
    var lines = std.mem.splitScalar(u8, input, '\n');
    var idx: usize = 0;
    var body_buf: [20000]u8 = undefined;
    while (lines.next()) |line| : (idx += 1) {
        if (line.len == 0) break;
        const body = try std.fmt.hexToBytes(&body_buf, line[1..]);
        var p: Parser = .init(std.testing.allocator);
        defer p.deinit();
        p.nextSlice("7501;");
        p.nextSlice(body);
        std.debug.print("TSDIFF {d}", .{idx});
        const cmd = p.end('\x1b') orelse {
            std.debug.print(" discard\n", .{});
            continue;
        };
        switch (cmd.*) {
            .program_status => |ps| switch (ps) {
                .query => std.debug.print(" query\n", .{}),
                .report => |r| {
                    std.debug.print(" report state={s}", .{@tagName(r.state)});
                    tsHex("id", r.readOption(.id));
                    if (r.readOption(.kind)) |k| std.debug.print(" kind={s}", .{@tagName(k)}) else std.debug.print(" kind=-", .{});
                    if (r.readOption(.progress)) |n| std.debug.print(" progress={d}", .{n}) else std.debug.print(" progress=-", .{});
                    tsHex("app", r.readOption(.app));
                    var tb: [max_msg_bytes]u8 = undefined;
                    var tw: std.Io.Writer = .fixed(&tb);
                    if (r.readOption(.title) != null) {
                        try r.writeText(.title, &tw);
                        tsHex("title", tw.buffered());
                    } else tsHex("title", null);
                    var mb: [max_msg_bytes]u8 = undefined;
                    var mw: std.Io.Writer = .fixed(&mb);
                    if (r.readOption(.msg) != null) {
                        try r.writeText(.msg, &mw);
                        tsHex("msg", mw.buffered());
                    } else tsHex("msg", null);
                    std.debug.print("\n", .{});
                },
            },
            else => std.debug.print(" other\n", .{}),
        }
    }
}
