function replace_line(ctx)
  client.output.replace("(1) " .. ctx.raw_line)
  client.echo("after replacement")
end

function gag_line(ctx)
  client.echo("numbered echo")
  client.output.gag()
end

function replace_then_gag(ctx)
  client.output.replace("not visible")
  client.output.gag()
end

function failed_edit(ctx)
  client.output.gag()
  error("failed edit")
end

function remember_original(ctx)
  client.var.set("observed", ctx.line)
end

function clear_then_replace(ctx)
  client.echo("evict source")
  client.echo("surviving echo")
  client.output.replace("must not replace echo")
end

function recurse_execute(ctx)
  client.execute("recursive")
end

function snapshot_original(ctx)
  client.var.set("snapshot", client.output.recent(1)[1])
end
