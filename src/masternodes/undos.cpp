// Copyright (c) DeFi Blockchain Developers
// Distributed under the MIT software license, see the accompanying
// file LICENSE or http://www.opensource.org/licenses/mit-license.php.

#include <masternodes/undos.h>
#include <masternodes/masternodes_common.h>

/// @attention make sure that it does not overlap with those in masternodes.cpp/tokens.cpp/undos.cpp/accounts.cpp !!!
const unsigned char CUndosView::ByUndoKey::prefix = PREFIX_CAST(DbPrefixes::DbPrefixesUndosByUndoKey);

void CUndosView::ForEachUndo(std::function<bool(UndoKey, CLazySerialize<CUndo>)> callback, UndoKey const & start) const
{
    ForEach<ByUndoKey, UndoKey, CUndo>(callback, start);
}

Res CUndosView::SetUndo(UndoKey key, CUndo const & undo)
{
    WriteBy<ByUndoKey>(key, undo);
    return Res::Ok();
}

Res CUndosView::DelUndo(UndoKey key)
{
    EraseBy<ByUndoKey>(key);
    return Res::Ok();
}

boost::optional<CUndo> CUndosView::GetUndo(UndoKey key) const
{
    CUndo val;
    bool ok = ReadBy<ByUndoKey>(key, val);
    if (ok) {
        return val;
    }
    return {};
}
