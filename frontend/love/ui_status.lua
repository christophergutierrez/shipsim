-- Shared status strip model (error / warn / info).

local ui_status = {}

function ui_status.new()
  return { level = "info", message = "", ticks = 0 }
end

function ui_status.set(st, level, message)
  st.level = level or "info"
  st.message = message or ""
  st.ticks = 0
end

function ui_status.from_error(st, err)
  if not err then
    return
  end
  ui_status.set(st, "error", err.message or err.code or "error")
end

function ui_status.clear(st)
  st.level = "info"
  st.message = ""
end

return ui_status
