import { Checkbox } from '@/components/ui/checkbox'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { ChevronDown, Check } from 'lucide-react'

// sentinel value:shadcn Select not allowed SelectItem use emptystringact value,
// so"do not bind"use oneitems sentinel placeholder,enter and exit onChange two way conversion at that time.
const NONE_VALUE = '__none__'

const NO_GROUPS_HINT_CLS =
  'text-xs text-muted-foreground italic px-1 py-2 leading-relaxed'

/** prompt the user to goGroup managementpageregister newGroup(replaces the old versionof"+ New group"inlineInput). */
function ManageGroupsHint() {
  return (
    <p className={NO_GROUPS_HINT_CLS}>
      No groups yet? Go to
      <a href="#/groups" className="text-primary underline mx-1">
        Group management
      </a>
      create. Group names must be registered first before they can be selected here, to avoid spelling drift.
    </p>
  )
}

/** single selectGroup:dropdown selects the currenthasGroup / do not bind.Used forClient key Bind group.
 *
 *  compared to before the changeofdifference:remove"+ New group"option(avoid typo drift).
 *  New groupplease go #/groups Managepage.
 */
export function GroupSingleSelect({
  value,
  options,
  onChange,
  disabled,
  noneLabel = '(unbound)',
}: {
  value: string
  options: string[]
  onChange: (v: string) => void
  disabled?: boolean
  noneLabel?: string
}) {
  // the current value is notinin the known options andnotempty → "alreadyDelete groupoflegacy reference"
  const isOrphan = value !== '' && !options.includes(value)
  // consistent with the whole siteof shadcn Select use NONE_VALUE sentinel in place of emptystring
  const selectValue = value === '' ? NONE_VALUE : value

  return (
    <div className="space-y-2">
      <Select
        value={selectValue}
        disabled={disabled}
        onValueChange={(v) => onChange(v === NONE_VALUE ? '' : v)}
      >
        <SelectTrigger className="h-10 rounded-xl px-3.5">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE_VALUE}>{noneLabel}</SelectItem>
          {options.map((g) => (
            <SelectItem key={g} value={g}>
              {g}
            </SelectItem>
          ))}
          {isOrphan && (
            <SelectItem value={value}>{value}(deactivated)</SelectItem>
          )}
        </SelectContent>
      </Select>
      {options.length === 0 && <ManageGroupsHint />}
      {isOrphan && (
        <p className="text-xs text-amber-600">
          Currently bound group &quot;{value}&quot; is no longer in the registry. Please reselect or go to
          <a href="#/groups" className="text-primary underline mx-1">
            Group management
          </a>
          recreate a group with the same name.
        </p>
      )}
    </div>
  )
}

/** multi selectGroup:dropdown menu form(Click to expand + multi select checkbox).Used forAccount(credential) groups Edit.
 *
 *  compared to before the changeofdifference:
 *  - when collapsed onlyShowoneitemsbutton,save space
 *  - multi select capability kept(onecredentialscan belong to many at oncegroups)
 *  - remove"+ New group"Inputbox,New groupplease go #/groups Managepage
 */
export function GroupMultiSelect({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string[]
  options: string[]
  onChange: (v: string[]) => void
  disabled?: boolean
}) {
  // option = registered ∪ currentSelected(includingmay be deregisteredofoldGroup,helps the userCancel)
  const allOptions = Array.from(new Set([...options, ...value])).sort()
  const orphans = value.filter((g) => !options.includes(g))

  const toggle = (g: string) => {
    if (value.includes(g)) onChange(value.filter((x) => x !== g))
    else onChange([...value, g])
  }

  // trigger buttonofdisplay text:not selected / Selected N items / singlegroupsdirectlyShowname
  const triggerLabel = (() => {
    if (value.length === 0) return 'Select group'
    if (value.length === 1) return value[0]
    return `Selected ${value.length} groups`
  })()

  return (
    <div className="space-y-2">
      {allOptions.length === 0 ? (
        <ManageGroupsHint />
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="outline"
              disabled={disabled}
              className="w-full justify-between font-normal"
            >
              <span className={value.length === 0 ? 'text-muted-foreground' : ''}>
                {triggerLabel}
              </span>
              <ChevronDown className="h-4 w-4 opacity-50" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            // with the trigger buttonetc.width,avoid a too narrow menuoroverflow
            style={{ width: 'var(--radix-dropdown-menu-trigger-width)' }}
            className="max-h-72 overflow-y-auto"
          >
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              Select groups (multi-select)
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            {allOptions.map((g) => {
              const orphan = !options.includes(g)
              const checked = value.includes(g)
              return (
                <DropdownMenuItem
                  key={g}
                  // preventDefault close-on-select:Group managementis a multi select case,after selecting oneitemsstill need to keep selecting
                  onSelect={(e) => {
                    e.preventDefault()
                    toggle(g)
                  }}
                  className="cursor-pointer gap-2"
                >
                  <Checkbox checked={checked} className="pointer-events-none" />
                  <span className={`flex-1 ${orphan ? 'italic text-amber-600' : ''}`}>
                    {g}
                    {orphan && '(deactivated)'}
                  </span>
                  {checked && <Check className="h-3.5 w-3.5 text-primary" />}
                </DropdownMenuItem>
              )
            })}
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="cursor-pointer text-xs text-muted-foreground"
              onSelect={(e) => {
                e.preventDefault()
                window.location.hash = '#/groups'
              }}
            >
              Manage groups…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {value.length > 0 && (
        <p className="text-xs text-muted-foreground">
          Selected:{value.join(',')}
        </p>
      )}
      {orphans.length > 0 && (
        <p className="text-xs text-amber-600">
          has {orphans.length} groups are no longer in the registry. Consider canceling or going to
          <a href="#/groups" className="text-primary underline mx-1">
            Group management
          </a>
          recreate.
        </p>
      )}
    </div>
  )
}
