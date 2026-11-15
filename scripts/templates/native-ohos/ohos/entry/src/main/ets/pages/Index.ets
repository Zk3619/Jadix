import native from 'libjadix_native_controls.so';
import { display, NodeContent } from '@kit.ArkUI';

@Entry
@Component
struct Index {
  private content: NodeContent = new NodeContent();
  private identity: string = '';
  private environment: number = 1;
  private pending: number = -1;
  @State failure: string = '';
  @State inspecting: boolean = false;
  @State includeText: boolean = false;
  @State snapshot: string = '';

  private inspect(): void {
    if (this.identity.length === 0) {
      this.snapshot = 'Native root is not mounted';
      return;
    }
    try {
      this.snapshot = native.nativeInspect(this.identity, this.includeText);
    } catch (error) {
      this.snapshot = String(error);
    }
  }

  private drain(identity: string): void {
    this.pending = -1;
    if (this.identity !== identity) {
      return;
    }
    try {
      native.nativePoll(identity);
    } catch (error) {
      this.failure = `${error}`;
    }
  }

  private resized(width: number, height: number): void {
    if (width <= 0 || height <= 0 || this.failure.length > 0) {
      return;
    }
    try {
      const density = display.getDefaultDisplaySync().densityPixels;
      if (this.identity.length === 0) {
        this.identity = native.nativeMount(this.content, width, height, density, () => {
          if (this.pending < 0) {
            const identity = this.identity;
            this.pending = setTimeout(() => this.drain(identity), 0);
          }
        });
      } else {
        this.environment += 1;
        native.nativeResize(this.identity, width, height, density, this.environment);
      }
    } catch (error) {
      this.failure = `${error}`;
    }
  }

  aboutToDisappear(): void {
    this.inspecting = false;
    this.includeText = false;
    this.snapshot = '';
    if (this.pending >= 0) {
      clearTimeout(this.pending);
      this.pending = -1;
    }
    if (this.identity.length > 0) {
      const identity = this.identity;
      this.identity = '';
      native.nativeUnmount(identity);
    }
  }

  build() {
    Stack() {
      ContentSlot(this.content)
      Column({ space: 8 }) {
        Button(this.inspecting ? 'Close inspector' : 'Inspect')
          .onClick(() => {
            this.inspecting = !this.inspecting;
            this.includeText = false;
            this.snapshot = '';
            if (this.inspecting) {
              this.inspect();
            }
          })
        if (this.inspecting) {
          Row({ space: 8 }) {
            Button('Refresh').onClick(() => this.inspect())
            Button(this.includeText ? 'Hide text' : 'Show text')
              .onClick(() => {
                this.includeText = !this.includeText;
                this.inspect();
              })
          }
          Text('Model metadata; platform accessibility requires separate inspection')
            .fontSize(12)
          Scroll() {
            Text(this.snapshot).fontSize(12).copyOption(CopyOptions.LocalDevice)
          }.height(240).width('100%')
        }
      }
      .padding(8).width(this.inspecting ? '100%' : 100)
      .backgroundColor(Color.White).align(Alignment.TopEnd)
      if (this.failure.length > 0) {
        Text(this.failure).fontColor(Color.Red).padding(16)
      }
    }
    .width('100%').height('100%').backgroundColor(Color.White)
    .onAreaChange((_previous, next) => this.resized(Number(next.width), Number(next.height)))
  }
}
