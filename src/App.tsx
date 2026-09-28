// 阶段 2 骨架占位界面：验证 React、Tailwind 与主题变量已接通。
// 正式的音量界面在阶段 3 实现。
export default function App() {
  return (
    <main className="flex h-screen flex-col items-center justify-center gap-2 bg-background text-foreground">
      <h1 className="text-lg font-semibold">Win11 声音控制器</h1>
      <p className="text-sm text-muted-foreground">项目骨架已就绪</p>
    </main>
  );
}
