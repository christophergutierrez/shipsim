function love.conf(t)
  t.identity = "shipsim"
  t.window.title = "shipsim"
  t.window.width = 1280
  t.window.height = 800
  t.window.resizable = true
  -- Prevent maximized-ultrawide short clients from clipping the allocate panel
  -- (Allocate button was below the fold at ~492px height).
  t.window.minwidth = 1024
  t.window.minheight = 720
  t.console = true
end
