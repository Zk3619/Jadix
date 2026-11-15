'use strict';

const {MiniProgramRuntime, environmentSnapshot} = require('../../jadix/runtime');

Page({
  data: {roots: [], error: ''},
  async onLoad() {
    this.unloaded = false;
    this.runtime = new MiniProgramRuntime(this, wx.createCanvasContext('jadix-measure', this));
    try {
      await this.runtime.load(WXWebAssembly, '/app.wasm.br');
      if (this.unloaded) return;
      const windowInfo = wx.getWindowInfo ? wx.getWindowInfo() : wx.getSystemInfoSync();
      this.runtime.mount(windowInfo.windowWidth, windowInfo.windowHeight);
      this.preferences = environmentSnapshot(wx.getSystemInfoSync(), windowInfo.theme);
      this.runtime.environment(this.preferences);
      this.themeListener = event => {
        if (this.unloaded) return;
        this.preferences = {...this.preferences, color_scheme: event.theme};
        try { this.runtime.environment(this.preferences); } catch (error) { this.report(error); }
      };
      if (wx.onThemeChange) wx.onThemeChange(this.themeListener);
    } catch (error) { if (!this.unloaded) this.report(error); }
  },
  report(error) { this.setData({error: String(error.message || error)}); },
  onJadixEvent(event) {
    try { if (this.runtime) this.runtime.send(event.detail); } catch (error) { this.report(error); }
  },
  onResize(event) {
    if (!this.runtime || !this.runtime.root || this.unloaded) return;
    try { this.runtime.resize(event.size.windowWidth, event.size.windowHeight); } catch (error) { this.report(error); }
  },
  onHide() {
    if (!this.preferences || this.unloaded) return;
    this.preferences = {...this.preferences, lifecycle: 'background'};
    try { this.runtime.environment(this.preferences); } catch (error) { this.report(error); }
  },
  onShow() {
    if (!this.preferences || this.unloaded) return;
    this.preferences = {...this.preferences, lifecycle: 'active'};
    try { this.runtime.environment(this.preferences); } catch (error) { this.report(error); }
  },
  onUnload() {
    this.unloaded = true;
    if (this.themeListener && wx.offThemeChange) wx.offThemeChange(this.themeListener);
    if (this.runtime) this.runtime.dispose();
  },
});
