# Store immutable Environments and durable assets

Every applied Environment is stored as an immutable snapshot backed by wallpaper content retained in a durable content-addressed store. This lets history replay the exact resolved palette, options, and wallpaper without consulting mutable Profiles, unavailable Sources, the network, or newer generation algorithms; discovery data and intermediate output remain disposable cache.
