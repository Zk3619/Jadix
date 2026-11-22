import {createApplication, createEmojiPicker, EmojiPicker} from '@jadix/shell';
const app = createApplication();
const picker = createEmojiPicker(app, {
  id: 'emoji', query: {tone: 5},
  onSelect(value) { console.log(value); },
  onChange() { app.mount(<EmojiPicker picker={picker} />); },
});
app.mount(<EmojiPicker picker={picker} style={{width: 340}} />);
picker.release();
