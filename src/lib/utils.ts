import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并 className，后出现的 Tailwind 类覆盖前面的冲突类。shadcn/ui 组件依赖此函数。 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
