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

  const topicVocabFallback = buildTopicVocab('Rust 编程词汇');

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
    wordSnakeTopic: {
      label: 'Word Snake Topic',
      config: () => {
        const config = snakeBase();
        const vocab = loadGeneratedVocab() || topicVocabFallback;
        config.title = `主题词表：${vocab.topic || '自定义'}`;
        config.dialogue = { start: '吃掉正确释义', win: '复习完成', lose: '撞到了，先歇一下' };
        config.rules.win_length = config.player.initial_length + vocab.target_correct;
        config.snake_vocab = vocab;
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

  const params = new URLSearchParams(location.search);
  const initialTopic = params.get('topic');
  if (initialTopic) {
    localStorage.setItem('bitcat.dev.topic_vocab', JSON.stringify(buildTopicVocab(initialTopic)));
  }
  const initialGame = params.get('game');
  const state = {
    selected: presets[initialGame] ? initialGame : (localStorage.getItem('bitcat.dev.game') || (initialTopic ? 'wordSnakeTopic' : 'wordSnake')),
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
        if (command === 'cmd_generate_word_snake_vocab') {
          const vocab = buildTopicVocab(args?.topic || 'Rust 编程词汇', args?.level || 'beginner');
          localStorage.setItem('bitcat.dev.topic_vocab', JSON.stringify(vocab));
          return Promise.resolve(vocab);
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
    const topicInput = document.getElementById('devTopicInput');
    const generate = document.getElementById('devGenerateBtn');
    if (!select || !restart) return;
    for (const [key, preset] of Object.entries(presets)) {
      const option = document.createElement('option');
      option.value = key;
      option.textContent = preset.label;
      select.appendChild(option);
    }
    select.value = state.selected;
    if (topicInput) topicInput.value = (loadGeneratedVocab() || topicVocabFallback).topic || 'Rust 编程词汇';
    select.addEventListener('change', () => {
      state.selected = select.value;
      localStorage.setItem('bitcat.dev.game', state.selected);
      location.reload();
    });
    if (generate && topicInput) {
      generate.addEventListener('click', async () => {
        const topic = topicInput.value.trim() || 'Rust 编程词汇';
        const vocab = await window.__TAURI__.core.invoke('cmd_generate_word_snake_vocab', { topic, level: 'beginner', count: 8 });
        localStorage.setItem('bitcat.dev.topic_vocab', JSON.stringify(vocab));
        localStorage.setItem('bitcat.dev.game', 'wordSnakeTopic');
        location.href = `${location.pathname}?game=wordSnakeTopic&rev=topic-${Date.now()}`;
      });
    }
    restart.addEventListener('click', () => {
      location.reload();
    });
  });

  function loadGeneratedVocab() {
    try {
      const raw = localStorage.getItem('bitcat.dev.topic_vocab');
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  }

  function buildTopicVocab(topic, level = 'beginner') {
    const cleanTopic = String(topic || '').trim() || 'Rust 编程词汇';
    const rust = /rust|编程|代码|program|code/i.test(cleanTopic);
    const meeting = /会议|沟通|meeting|communicat/i.test(cleanTopic);
    const entries = rust ? [
      {
        id: 'ownership',
        term: 'ownership',
        meaning: '所有权',
        distractors: ['继承', '窗口', '循环'],
        example: 'Ownership helps Rust manage memory safely.',
        hint: '常和 memory、borrow 一起出现',
        explanation: 'ownership 指资源由谁负责管理，不是 inheritance 那种继承关系。',
      },
      {
        id: 'borrow',
        term: 'borrow',
        meaning: '借用',
        distractors: ['删除', '部署', '排序'],
        example: 'You can borrow a value without taking ownership.',
        hint: 'Rust 里常见 borrow checker',
        explanation: 'borrow 是临时使用值，ownership 才是拿走所有权。',
      },
      {
        id: 'lifetime',
        term: 'lifetime',
        meaning: '生命周期',
        distractors: ['文件路径', '错误码', '颜色主题'],
        example: 'A lifetime tells Rust how long a reference is valid.',
        hint: '和 reference 有关',
        explanation: 'lifetime 说明引用有效多久，不是程序运行速度。',
      },
      {
        id: 'trait',
        term: 'trait',
        meaning: '特征、接口能力',
        distractors: ['变量名', '测试报告', '压缩包'],
        example: 'A trait defines shared behavior for different types.',
        hint: '像一组可实现的能力',
        explanation: 'trait 描述类型能做什么，不是某个具体变量。',
      },
    ] : meeting ? [
      {
        id: 'clarify',
        term: 'clarify',
        meaning: '澄清、说明清楚',
        distractors: ['取消', '隐藏', '复制'],
        example: 'Could you clarify the next step before we decide?',
        hint: '常用于问题还不够清楚时',
        explanation: 'clarify 是把事情讲清楚，不是取消讨论。',
      },
      {
        id: 'priority',
        term: 'priority',
        meaning: '优先级',
        distractors: ['地点', '噪音', '装饰'],
        example: 'The launch bug is our top priority today.',
        hint: '和 top、urgent、important 常一起出现',
        explanation: 'priority 表示先处理什么，不是任务地点。',
      },
      {
        id: 'follow-up',
        term: 'follow-up',
        meaning: '后续跟进',
        distractors: ['立即删除', '随意猜测', '安静等待'],
        example: 'I will send a follow-up after the meeting.',
        hint: '会后继续处理',
        explanation: 'follow-up 是会后继续跟进，不是现在停止。',
      },
      {
        id: 'decision',
        term: 'decision',
        meaning: '决定、决策',
        distractors: ['天气', '入口', '礼物'],
        example: 'We need a decision before Friday.',
        hint: '需要选择一个方向',
        explanation: 'decision 是做出选择，不是普通提醒。',
      },
    ] : [
      {
        id: 'explore',
        term: 'explore',
        meaning: '探索',
        distractors: ['拒绝', '重复', '隐藏'],
        example: `Let's explore useful words about ${cleanTopic}.`,
        hint: '开始了解一个主题',
        explanation: 'explore 是主动探索主题，不是拒绝它。',
      },
      {
        id: 'practice',
        term: 'practice',
        meaning: '练习',
        distractors: ['遗忘', '删除', '装饰'],
        example: `Short practice makes ${cleanTopic} easier to remember.`,
        hint: '反复做来变熟',
        explanation: 'practice 是通过重复变熟，不是把内容删掉。',
      },
      {
        id: 'context',
        term: 'context',
        meaning: '语境、上下文',
        distractors: ['价格', '门票', '颜色'],
        example: 'The context helps you choose the right meaning.',
        hint: '理解句子的背景',
        explanation: 'context 是帮助理解的语境，不是孤立的价格或颜色。',
      },
      {
        id: 'review',
        term: 'review',
        meaning: '复习、回顾',
        distractors: ['逃跑', '购买', '折叠'],
        example: `Review ${cleanTopic} words after one round.`,
        hint: '学完以后再看一遍',
        explanation: 'review 是回头复习，不是离开或购买。',
      },
    ];
    return {
      topic: cleanTopic,
      level,
      mode: 'meaning_choice',
      answer_count: 4,
      target_correct: Math.min(8, entries.length * 2),
      entries,
    };
  }
})();
