export type RootType = "asset" | "equity" | "expense" | "income" | "liability";

export type AccountType =
  | "accumulated_depreciation"
  | "bank"
  | "cash"
  | "cogs"
  | "depreciation"
  | "expense"
  | "fixed_asset"
  | "income"
  | "payable"
  | "receivable"
  | "round_off"
  | "stock"
  | "stock_adjustment"
  | "tax"
  | "temporary";

export type ChartAccount = {
  accountType: AccountType;
  code: string;
  currency: string;
  id: string;
  isGroup?: boolean;
  name: string;
  parentId?: string;
  rootType: RootType;
};

export type ChartOfAccounts = {
  accounts: ChartAccount[];
  id: "skr03" | "skr04";
  label: string;
};
