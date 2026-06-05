(function () {
  const vocabBasic = {
    mode: 'meaning_choice',
    answer_count: 4,
    target_correct: 12,
    entries: [
      { id: 'absorb', term: 'absorb', meaning: '吸收', distractors: ['逃避', '预测', '修复'], example: 'Plants absorb water through roots.' },
      { id: 'adjust', term: 'adjust', meaning: '调整', distractors: ['拒绝', '复制', '删除'], example: 'Adjust the volume before the meeting starts.' },
      { id: 'compare', term: 'compare', meaning: '比较', distractors: ['隐藏', '购买', '忘记'], example: 'Compare the two answers before choosing.' },
      { id: 'create', term: 'create', meaning: '创造', distractors: ['取消', '等待', '借用'], example: 'You can create a new note from the panel.' },
      { id: 'focus', term: 'focus', meaning: '专注', distractors: ['庆祝', '移动', '猜测'], example: 'Focus on the current task first.' },
      { id: 'review', term: 'review', meaning: '复习', distractors: ['删除', '奔跑', '摘录'], example: 'Review the words you missed yesterday.' },
      { id: 'select', term: 'select', meaning: '选择', distractors: ['推迟', '描绘', '沉默'], example: 'Select the correct answer.' },
      { id: 'mistake', term: 'mistake', meaning: '错误', distractors: ['礼物', '入口', '天气'], example: 'A mistake is a chance to practice again.' },
    ],
  };

  const vocabLong = {
    ...vocabBasic,
    target_correct: 8,
    entries: [
      {
        id: 'responsibility',
        term: 'responsibility',
        meaning: '责任、职责、需要承担的事情',
        distractors: ['临时的想法和猜测', '快速跳过当前任务', '完全无关的装饰物'],
        example: 'Taking responsibility means you own the result and communicate clearly.',
      },
      {
        id: 'misunderstanding',
        term: 'misunderstanding',
        meaning: '误解、理解错了对方意思',
        distractors: ['明显正确的答案', '安静地等待', '短暂休息'],
        example: 'A misunderstanding can be fixed by asking one careful question.',
      },
      {
        id: 'concentration',
        term: 'concentration',
        meaning: '专注、集中注意力',
        distractors: ['随意切换窗口', '准备晚饭', '隐藏按钮'],
        example: 'Concentration is easier when the screen is not noisy.',
      },
      {
        id: 'implementation',
        term: 'implementation',
        meaning: '实现、把计划做成可运行结果',
        distractors: ['取消项目', '只写标题', '颜色变浅'],
        example: 'The implementation should match the design goal.',
      },
    ],
  };

  function snakeBase() {
    return {
      game_type: 'snake',
      title: '毛线球大作战',
      grid: { width: 48, height: 32, cell_size: 16 },
      player: { speed_ms: 95, initial_length: 5 },
      rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.975, win_length: 140 },
      theme: { head: 'cat', body: 'yarn', food: 'mouse', trail_alpha: 0.55 },
      dialogue: { start: '喵！看我的！', win: '太厉害了喵~', lose: '呜...再来一次！' },
    };
  }

  const presets = {
    snake: {
      label: 'Snake',
      config: () => snakeBase(),
    },
    wordSnake: {
      label: 'Word Snake',
      config: () => {
        const config = snakeBase();
        config.title = '单词贪吃蛇';
        config.dialogue = { start: '吃掉正确释义', win: '复习完成', lose: '撞到了，先歇一下' };
        config.rules.win_length = config.player.initial_length + vocabBasic.target_correct;
        config.snake_vocab = vocabBasic;
        return config;
      },
    },
    wordSnakeLong: {
      label: 'Word Snake Long Text',
      config: () => {
        const config = snakeBase();
        config.title = '长文本单词贪吃蛇';
        config.dialogue = { start: '吃掉正确释义', win: '复习完成', lose: '撞到了，先歇一下' };
        config.rules.win_length = config.player.initial_length + vocabLong.target_correct;
        config.snake_vocab = vocabLong;
        return config;
      },
    },
    memory: {
      label: 'Memory',
      config: () => ({
        game_type: 'memory',
        title: 'Memory Match',
        grid: { width: 4, height: 4, cell_size: 96 },
        player: { speed_ms: 140, initial_length: 3 },
        rules: { walls_kill: false, self_kill: false, food_count: 1, speed_ramp: 0.95, win_length: 16 },
        theme: { head: 'cat', body: 'yarn', food: 'fish', trail_alpha: 0.55 },
        dialogue: { start: 'Find every pair', win: 'All matched', lose: 'Try again' },
      }),
    },
    catch: {
      label: 'Catch',
      config: () => ({
        game_type: 'catch',
        title: 'Catch Treats',
        grid: { width: 24, height: 16, cell_size: 32 },
        player: { speed_ms: 180, initial_length: 3 },
        rules: { walls_kill: false, self_kill: false, food_count: 1, speed_ramp: 0.97, win_length: 30 },
        theme: { head: 'cat', body: 'dot', food: 'fish', trail_alpha: 0.55 },
        dialogue: { start: 'Catch the treats', win: 'Nice catch', lose: 'Missed too many' },
      }),
    },
    battle: {
      label: 'Battle',
      config: () => ({
        game_type: 'battle',
        title: '守护召唤战',
        grid: { width: 30, height: 20, cell_size: 24 },
        player: { speed_ms: 140, initial_length: 3 },
        rules: { walls_kill: true, self_kill: true, food_count: 1, speed_ramp: 0.95, win_length: 50 },
        theme: { head: 'cat', body: 'yarn', food: 'mouse', trail_alpha: 0.55 },
        dialogue: { start: '传送门打开了，帮我一起打！', win: '赢啦！材料到手！', lose: '呜...下次我会更强。' },
      }),
    },
  };

  const initialGame = new URLSearchParams(location.search).get('game');
  const state = {
    selected: presets[initialGame] ? initialGame : (localStorage.getItem('bitcat.dev.game') || 'wordSnake'),
    listeners: new Map(),
  };

  function currentConfig() {
    return structuredClone((presets[state.selected] || presets.wordSnake).config());
  }

  function emit(name, payload) {
    const handlers = state.listeners.get(name) || [];
    for (const handler of handlers) handler({ payload });
  }

  window.__TAURI__ = {
    core: {
      invoke(command, args) {
        if (command === 'cmd_get_current_game') return Promise.resolve(currentConfig());
        if (command === 'cmd_game_log') {
          console.debug('[game-dev]', args && args.msg);
          return Promise.resolve();
        }
        if (command === 'cmd_game_end') {
          console.debug('[game-dev] end', args);
          return Promise.resolve();
        }
        if (command === 'cmd_game_set_input_capture') return Promise.resolve();
        if (command === 'cmd_game_cursor_position') return Promise.resolve({ x: -1, y: -1 });
        return Promise.resolve(null);
      },
    },
    event: {
      listen(name, handler) {
        const handlers = state.listeners.get(name) || [];
        handlers.push(handler);
        state.listeners.set(name, handlers);
        return Promise.resolve(() => {
          state.listeners.set(name, (state.listeners.get(name) || []).filter((item) => item !== handler));
        });
      },
    },
  };

  window.BitCatGameDev = { presets, state, currentConfig, emit };

  document.addEventListener('DOMContentLoaded', () => {
    const select = document.getElementById('devGameSelect');
    const restart = document.getElementById('devRestartBtn');
    if (!select || !restart) return;
    for (const [key, preset] of Object.entries(presets)) {
      const option = document.createElement('option');
      option.value = key;
      option.textContent = preset.label;
      select.appendChild(option);
    }
    select.value = state.selected;
    select.addEventListener('change', () => {
      state.selected = select.value;
      localStorage.setItem('bitcat.dev.game', state.selected);
      location.reload();
    });
    restart.addEventListener('click', () => {
      location.reload();
    });
  });
})();
